use std::{
    io::{BufRead, BufReader},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use anyhow::bail;
use async_trait::async_trait;
use serde_json::{Value, json};
use tracing::debug;

use crate::{
    model::types::{MessageRole, ProviderEvent, ToolCall},
    providers::provider::{ModelProvider, ProviderTurn, ProviderTurnRequest, TurnHandle},
};

#[derive(Debug, Clone)]
pub struct OpenAiResponsesProvider {
    base_url: String,
}

impl OpenAiResponsesProvider {
    const DEFAULT_INSTRUCTIONS: &'static str =
        "You are Maky CLI, a helpful coding assistant. Respond concisely and clearly.";

    fn user_agent_header() -> String {
        format!(
            "maky-cli/{} ({} {}; {})",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            "rust"
        )
    }

    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }

    fn chunked_response_text(&self, request: &ProviderTurnRequest) -> String {
        let latest_user_message = request
            .messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| message.content.as_str())
            .unwrap_or("No user prompt was provided.");

        format!(
            "OpenAI streaming provider active via {}. model={} variant={}. Echo: {}",
            self.base_url, request.model, request.variant, latest_user_message
        )
    }

    fn stream_stubbed_response(
        cancellation_flag: Arc<AtomicBool>,
        event_tx: mpsc::Sender<ProviderEvent>,
        response_text: String,
    ) {
        for token in response_text.split_whitespace() {
            if cancellation_flag.load(Ordering::SeqCst) {
                let _ = event_tx.send(ProviderEvent::Cancelled);
                return;
            }

            let delta = format!("{token} ");
            if event_tx.send(ProviderEvent::TextDelta(delta)).is_err() {
                return;
            }

            thread::sleep(Duration::from_millis(35));
        }

        if cancellation_flag.load(Ordering::SeqCst) {
            let _ = event_tx.send(ProviderEvent::Cancelled);
            return;
        }

        let _ = event_tx.send(ProviderEvent::Completed);
    }

    fn stream_live_response(
        request: ProviderTurnRequest,
        cancellation_flag: Arc<AtomicBool>,
        event_tx: mpsc::Sender<ProviderEvent>,
    ) {
        if cancellation_flag.load(Ordering::SeqCst) {
            let _ = event_tx.send(ProviderEvent::Cancelled);
            return;
        }

        let Some(access_token) = request.auth.access_token.clone() else {
            let _ = event_tx.send(ProviderEvent::Error(
                "missing access token for provider request".to_string(),
            ));
            return;
        };

        let endpoint = Self::codex_responses_endpoint().to_string();
        let payload = Self::build_responses_payload(&request);

        debug!(
            endpoint,
            model = request.model,
            variant = request.variant,
            "starting openai responses stream"
        );

        let client = match reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                let _ = event_tx.send(ProviderEvent::Error(format!(
                    "failed to build OpenAI HTTP client: {error}"
                )));
                return;
            }
        };

        let mut request_builder = client
            .post(endpoint.clone())
            .bearer_auth(access_token)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .header(reqwest::header::USER_AGENT, Self::user_agent_header())
            .header("originator", "maky-cli")
            .header("session_id", request.session_id.as_str());

        if let Some(account_id) = request.auth.account_id.as_deref() {
            request_builder = request_builder.header("ChatGPT-Account-Id", account_id);
        }

        let response_result = request_builder.json(&payload).send();

        let response = match response_result {
            Ok(response) => response,
            Err(error) => {
                let _ = event_tx.send(ProviderEvent::Error(format!(
                    "failed to send OpenAI request to {endpoint}: {error}"
                )));
                return;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            let body_snippet = Self::first_non_empty_line(&body)
                .map(|line| Self::truncate(line, 240))
                .unwrap_or_else(|| "<empty body>".to_string());
            let auth_hint = if status == reqwest::StatusCode::UNAUTHORIZED {
                if request
                    .auth
                    .source
                    .trim()
                    .eq_ignore_ascii_case("oauth-session")
                {
                    " OAuth session rejected by provider. Re-run /login."
                } else {
                    " API key rejected by provider. Check key and model access."
                }
            } else {
                ""
            };
            let _ = event_tx.send(ProviderEvent::Error(format!(
                "OpenAI request failed with status {status} at {endpoint}: {body_snippet}{auth_hint}"
            )));
            return;
        }

        let mut reader = BufReader::new(response);
        let mut line = String::new();
        let mut data_lines = Vec::new();
        let mut completed_sent = false;

        loop {
            if cancellation_flag.load(Ordering::SeqCst) {
                let _ = event_tx.send(ProviderEvent::Cancelled);
                return;
            }

            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim_end_matches(&['\r', '\n'][..]);
                    if trimmed.is_empty() {
                        if data_lines.is_empty() {
                            continue;
                        }

                        let data_block = data_lines.join("\n");
                        data_lines.clear();
                        match Self::process_sse_data_block(
                            &data_block,
                            &event_tx,
                            &mut completed_sent,
                        ) {
                            StreamDirective::Continue => {}
                            StreamDirective::Completed | StreamDirective::Failed => return,
                        }
                        continue;
                    }

                    if let Some(data) = trimmed.strip_prefix("data:") {
                        data_lines.push(data.trim_start().to_string());
                    }
                }
                Err(error) => {
                    let _ = event_tx.send(ProviderEvent::Error(format!(
                        "failed while reading OpenAI stream: {error}"
                    )));
                    return;
                }
            }
        }

        if !data_lines.is_empty() {
            let data_block = data_lines.join("\n");
            match Self::process_sse_data_block(&data_block, &event_tx, &mut completed_sent) {
                StreamDirective::Continue => {}
                StreamDirective::Completed | StreamDirective::Failed => return,
            }
        }

        if cancellation_flag.load(Ordering::SeqCst) {
            let _ = event_tx.send(ProviderEvent::Cancelled);
            return;
        }

        if !completed_sent {
            let _ = event_tx.send(ProviderEvent::Completed);
        }
    }

    fn process_sse_data_block(
        data_block: &str,
        event_tx: &mpsc::Sender<ProviderEvent>,
        completed_sent: &mut bool,
    ) -> StreamDirective {
        let data_block = data_block.trim();
        if data_block.is_empty() {
            return StreamDirective::Continue;
        }

        if data_block == "[DONE]" {
            if !*completed_sent {
                if event_tx.send(ProviderEvent::Completed).is_err() {
                    return StreamDirective::Failed;
                }
                *completed_sent = true;
            }
            return StreamDirective::Completed;
        }

        let payload: Value = match serde_json::from_str(data_block) {
            Ok(payload) => payload,
            Err(error) => {
                let _ = event_tx.send(ProviderEvent::Error(format!(
                    "failed to parse OpenAI stream event as JSON: {error}"
                )));
                return StreamDirective::Failed;
            }
        };

        if let Some(delta) = Self::extract_text_delta(&payload)
            && !delta.is_empty()
            && event_tx.send(ProviderEvent::TextDelta(delta)).is_err()
        {
            return StreamDirective::Failed;
        }

        if let Some(tool_call) = Self::extract_tool_call(&payload)
            && event_tx
                .send(ProviderEvent::ToolCallRequested(tool_call))
                .is_err()
        {
            return StreamDirective::Failed;
        }

        let event_type = payload
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match event_type {
            "response.completed" => {
                if !*completed_sent {
                    if event_tx.send(ProviderEvent::Completed).is_err() {
                        return StreamDirective::Failed;
                    }
                    *completed_sent = true;
                }
                StreamDirective::Completed
            }
            "response.failed" | "error" => {
                let message = Self::extract_error_message(&payload);
                let _ = event_tx.send(ProviderEvent::Error(message));
                StreamDirective::Failed
            }
            _ => StreamDirective::Continue,
        }
    }

    fn extract_text_delta(payload: &Value) -> Option<String> {
        let event_type = payload
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if event_type != "response.output_text.delta" {
            return None;
        }

        payload
            .get("delta")
            .and_then(Value::as_str)
            .map(ToString::to_string)
    }

    fn extract_tool_call(payload: &Value) -> Option<ToolCall> {
        let event_type = payload
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if event_type != "response.output_item.added" {
            return None;
        }

        let item = payload.get("item")?;
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            return None;
        }

        let id = item
            .get("id")
            .and_then(Value::as_str)
            .or_else(|| item.get("call_id").and_then(Value::as_str))
            .unwrap_or("tool-call")
            .to_string();
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unknown_tool")
            .to_string();
        let args = item
            .get("arguments")
            .and_then(Value::as_str)
            .map(|arguments| vec![arguments.to_string()])
            .unwrap_or_default();

        Some(ToolCall { id, name, args })
    }

    fn extract_error_message(payload: &Value) -> String {
        payload
            .pointer("/error/message")
            .and_then(Value::as_str)
            .or_else(|| payload.get("message").and_then(Value::as_str))
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                let event_type = payload
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown-error");
                format!("OpenAI stream failed with event `{event_type}`")
            })
    }

    fn codex_responses_endpoint() -> &'static str {
        "https://chatgpt.com/backend-api/codex/responses"
    }

    fn build_responses_payload(request: &ProviderTurnRequest) -> Value {
        let input_messages = request
            .messages
            .iter()
            .map(|message| {
                json!({
                    "role": Self::provider_role(message.role),
                    "content": message.content,
                })
            })
            .collect::<Vec<_>>();
        let instructions = Self::build_instructions(request);

        let mut payload = json!({
            "model": Self::provider_model(&request.model),
            "instructions": instructions,
            "input": input_messages,
            "stream": true,
            "store": false,
        });

        if let Some(effort) = Self::reasoning_effort(&request.variant) {
            payload["reasoning"] = json!({ "effort": effort });
        }

        payload
    }

    fn build_instructions(request: &ProviderTurnRequest) -> String {
        let system_instructions = request
            .messages
            .iter()
            .filter(|message| message.role == MessageRole::System)
            .map(|message| message.content.trim())
            .filter(|message| !message.is_empty())
            .collect::<Vec<_>>();

        if system_instructions.is_empty() {
            Self::DEFAULT_INSTRUCTIONS.to_string()
        } else {
            system_instructions.join("\n\n")
        }
    }

    fn provider_role(role: MessageRole) -> &'static str {
        match role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
            MessageRole::System => "system",
        }
    }

    fn provider_model(model: &str) -> String {
        model
            .strip_prefix("openai/")
            .unwrap_or(model)
            .trim()
            .to_string()
    }

    fn reasoning_effort(variant: &str) -> Option<&'static str> {
        match variant.trim().to_ascii_lowercase().as_str() {
            "none" => Some("none"),
            "minimal" => Some("minimal"),
            "low" => Some("low"),
            "medium" => Some("medium"),
            "high" => Some("high"),
            "xhigh" => Some("xhigh"),
            _ => None,
        }
    }

    fn first_non_empty_line(body: &str) -> Option<&str> {
        body.lines().map(str::trim).find(|line| !line.is_empty())
    }

    fn truncate(value: &str, max_chars: usize) -> String {
        if value.chars().count() <= max_chars {
            return value.to_string();
        }

        let mut truncated = value.chars().take(max_chars).collect::<String>();
        truncated.push_str("...");
        truncated
    }
}

impl Default for OpenAiResponsesProvider {
    fn default() -> Self {
        Self::new("https://chatgpt.com/backend-api/codex/responses")
    }
}

#[async_trait]
impl ModelProvider for OpenAiResponsesProvider {
    fn id(&self) -> &'static str {
        "openai-responses"
    }

    async fn stream_turn(&self, request: ProviderTurnRequest) -> anyhow::Result<ProviderTurn> {
        if request.messages.is_empty() {
            bail!("provider request must include at least one message")
        }

        let is_mock_mode = self.base_url.starts_with("mock://");
        let is_oauth_session = request
            .auth
            .source
            .trim()
            .eq_ignore_ascii_case("oauth-session");

        if !is_mock_mode && !is_oauth_session {
            let (event_tx, event_rx) = mpsc::channel();
            let handle = TurnHandle::new(request.turn_id.clone());
            let source = request.auth.source;
            let _ = event_tx.send(ProviderEvent::Error(format!(
                "Codex provider requires OAuth session credentials (source={source}). Run /login."
            )));
            return Ok(ProviderTurn {
                turn_id: request.turn_id,
                event_rx,
                handle,
            });
        }

        if request.auth.access_token.is_none() {
            let (event_tx, event_rx) = mpsc::channel();
            let handle = TurnHandle::new(request.turn_id.clone());
            let source = request.auth.source;
            let status = request.auth.status;
            let provider_id = request.auth.provider_id;
            let _ = event_tx.send(ProviderEvent::Error(format!(
                "missing access token for provider request (source={source}, status={status}, provider={provider_id})"
            )));
            return Ok(ProviderTurn {
                turn_id: request.turn_id,
                event_rx,
                handle,
            });
        }

        let (event_tx, event_rx) = mpsc::channel();
        let turn_id = request.turn_id.clone();
        let handle = TurnHandle::new(turn_id.clone());
        let cancellation_flag = handle.cancellation_flag();
        let base_url = self.base_url.clone();

        thread::spawn(move || {
            if base_url.starts_with("mock://") {
                let response_text = Self {
                    base_url: base_url.clone(),
                }
                .chunked_response_text(&request);
                Self::stream_stubbed_response(cancellation_flag, event_tx, response_text);
            } else {
                Self::stream_live_response(request, cancellation_flag, event_tx);
            }
        });

        Ok(ProviderTurn {
            turn_id,
            event_rx,
            handle,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamDirective {
    Continue,
    Completed,
    Failed,
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, time::Duration};

    use super::*;
    use crate::{
        model::types::{Message, ProviderEvent},
        providers::provider::ProviderAuthContext,
        util::block_on_future,
    };

    fn request_with_access_token(access_token: Option<&str>) -> ProviderTurnRequest {
        ProviderTurnRequest {
            turn_id: "turn-1".to_string(),
            session_id: "session-1".to_string(),
            model: "openai/gpt-5.3-codex".to_string(),
            variant: "medium".to_string(),
            messages: vec![Message {
                id: "message-1".to_string(),
                role: MessageRole::User,
                content: "hello".to_string(),
                timestamp: 1,
            }],
            auth: ProviderAuthContext {
                access_token: access_token.map(ToString::to_string),
                source: "oauth-session".to_string(),
                status: "signed-in".to_string(),
                provider_id: "chatgpt-oauth".to_string(),
                account_id: Some("test-account".to_string()),
            },
        }
    }

    #[test]
    fn codex_endpoint_is_fixed() {
        assert_eq!(
            OpenAiResponsesProvider::codex_responses_endpoint(),
            "https://chatgpt.com/backend-api/codex/responses"
        );
    }

    #[test]
    fn normalizes_provider_prefixed_model_ids() {
        assert_eq!(
            OpenAiResponsesProvider::provider_model("openai/gpt-5.3-codex"),
            "gpt-5.3-codex"
        );
        assert_eq!(
            OpenAiResponsesProvider::provider_model("gpt-4.1-mini"),
            "gpt-4.1-mini"
        );
    }

    #[test]
    fn xhigh_variant_maps_to_xhigh_effort() {
        let mut request = request_with_access_token(Some("token"));
        request.variant = "xhigh".to_string();

        let payload = OpenAiResponsesProvider::build_responses_payload(&request);
        assert_eq!(
            payload.pointer("/reasoning/effort").and_then(Value::as_str),
            Some("xhigh")
        );
    }

    #[test]
    fn payload_includes_default_instructions() {
        let request = request_with_access_token(Some("token"));

        let payload = OpenAiResponsesProvider::build_responses_payload(&request);
        assert_eq!(
            payload.pointer("/instructions").and_then(Value::as_str),
            Some(OpenAiResponsesProvider::DEFAULT_INSTRUCTIONS)
        );
        assert_eq!(payload.pointer("/store").and_then(Value::as_bool), Some(false));
    }

    #[test]
    fn payload_uses_system_messages_as_instructions() {
        let mut request = request_with_access_token(Some("token"));
        request.messages.insert(
            0,
            Message {
                id: "system-message-1".to_string(),
                role: MessageRole::System,
                content: "Follow repo conventions.".to_string(),
                timestamp: 0,
            },
        );

        let payload = OpenAiResponsesProvider::build_responses_payload(&request);
        assert_eq!(
            payload.pointer("/instructions").and_then(Value::as_str),
            Some("Follow repo conventions.")
        );
    }

    #[test]
    fn codex_mode_rejects_non_oauth_credentials() {
        let provider =
            OpenAiResponsesProvider::new("https://chatgpt.com/backend-api/codex/responses");
        let mut request = request_with_access_token(Some("api-key-token"));
        request.auth.source = "api-key".to_string();

        let turn = block_on_future(provider.stream_turn(request)).expect("turn should initialize");
        let event = turn
            .event_rx
            .recv_timeout(Duration::from_millis(250))
            .expect("non-oauth credentials should emit an error event");

        assert!(
            matches!(event, ProviderEvent::Error(message) if message.contains("requires OAuth session"))
        );
    }

    #[test]
    fn emits_text_delta_for_output_text_delta_events() {
        let (event_tx, event_rx) = mpsc::channel();
        let mut completed_sent = false;

        let directive = OpenAiResponsesProvider::process_sse_data_block(
            r#"{"type":"response.output_text.delta","delta":"hello"}"#,
            &event_tx,
            &mut completed_sent,
        );

        assert_eq!(directive, StreamDirective::Continue);
        assert_eq!(
            event_rx.try_recv().expect("delta event should be emitted"),
            ProviderEvent::TextDelta("hello".to_string())
        );
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn done_marker_emits_completed_once() {
        let (event_tx, event_rx) = mpsc::channel();
        let mut completed_sent = false;

        let first = OpenAiResponsesProvider::process_sse_data_block(
            "[DONE]",
            &event_tx,
            &mut completed_sent,
        );
        let second = OpenAiResponsesProvider::process_sse_data_block(
            "[DONE]",
            &event_tx,
            &mut completed_sent,
        );

        assert_eq!(first, StreamDirective::Completed);
        assert_eq!(second, StreamDirective::Completed);
        assert_eq!(
            event_rx
                .try_recv()
                .expect("first done marker should complete the turn"),
            ProviderEvent::Completed
        );
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn missing_access_token_returns_error_event() {
        let provider = OpenAiResponsesProvider::new("mock://openai");
        let request = request_with_access_token(None);

        let turn = block_on_future(provider.stream_turn(request)).expect("turn should initialize");
        let event = turn
            .event_rx
            .recv_timeout(Duration::from_millis(250))
            .expect("missing token should emit an error event");

        assert!(
            matches!(event, ProviderEvent::Error(message) if message.contains("missing access token"))
        );
    }

    #[test]
    fn mock_base_url_streams_stubbed_text() {
        let provider = OpenAiResponsesProvider::new("mock://openai");
        let request = request_with_access_token(Some("local-test-token"));

        let turn = block_on_future(provider.stream_turn(request)).expect("turn should initialize");
        let mut saw_delta = false;
        let mut saw_completed = false;

        for _ in 0..2_000 {
            match turn.event_rx.recv_timeout(Duration::from_millis(5)) {
                Ok(ProviderEvent::TextDelta(_)) => saw_delta = true,
                Ok(ProviderEvent::Completed) => {
                    saw_completed = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }

        assert!(
            saw_delta,
            "stubbed stream should emit at least one text delta"
        );
        assert!(
            saw_completed,
            "stubbed stream should terminate with completed"
        );
    }
}
