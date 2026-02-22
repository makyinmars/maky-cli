use std::{
    sync::{Arc, atomic::Ordering, mpsc},
    thread,
    time::Duration,
};

use anyhow::bail;
use async_trait::async_trait;

use crate::{
    model::types::{MessageRole, ProviderEvent},
    providers::provider::{ModelProvider, ProviderTurn, ProviderTurnRequest, TurnHandle},
};

#[derive(Debug, Clone)]
pub struct OpenAiResponsesProvider {
    base_url: String,
}

impl OpenAiResponsesProvider {
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
        cancellation_flag: Arc<std::sync::atomic::AtomicBool>,
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
}

impl Default for OpenAiResponsesProvider {
    fn default() -> Self {
        Self::new("https://api.openai.com/v1")
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

        let response_text = self.chunked_response_text(&request);
        let (event_tx, event_rx) = mpsc::channel();
        let handle = TurnHandle::new(request.turn_id.clone());
        let cancellation_flag = handle.cancellation_flag();

        thread::spawn(move || {
            Self::stream_stubbed_response(cancellation_flag, event_tx, response_text)
        });

        Ok(ProviderTurn {
            turn_id: request.turn_id,
            event_rx,
            handle,
        })
    }
}
