use std::{
    sync::mpsc::{Receiver, TryRecvError},
    time::Duration,
};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::Backend};
use tracing::{error, info, warn};

use crate::{
    app::{
        event::{AppEvent, EventSource},
        state::{
            AppState, CANCEL_SHORTCUT_LABEL, InputMode, LOCAL_COMMANDS_INLINE_NO_QUIT,
            LOGIN_COMMAND_USAGE, LocalCommand, LoginCommandMode, MessageRole, TurnState,
            parse_local_command,
        },
        ui,
    },
    auth::{AuthRuntime, provider::AuthLoginMethod},
    model::types::{Message as ProviderMessage, ProviderEvent},
    providers::{
        openai_responses::OpenAiResponsesProvider,
        provider::{ModelProvider, ProviderAuthContext, ProviderTurnRequest, TurnHandle},
    },
    storage::config::AppConfig,
    util::{block_on_future, unix_timestamp_secs},
};

fn local_help_message() -> String {
    format!(
        "Local commands:\n/help - show this help\n{LOGIN_COMMAND_USAGE} - start OAuth login\n/auth - show auth status\n/logout - clear persisted OAuth session\n/cancel - cancel the active provider turn\n/quit - exit the app\nShortcut: {CANCEL_SHORTCUT_LABEL} also cancels an active turn."
    )
}

struct ActiveProviderTurn {
    event_rx: Receiver<ProviderEvent>,
    handle: TurnHandle,
}

enum ProviderPollResult {
    Event(ProviderEvent),
    Disconnected,
    Empty,
}

pub struct AppController {
    state: AppState,
    events: EventSource,
    auth: AuthRuntime,
    provider: Box<dyn ModelProvider>,
    active_provider_turn: Option<ActiveProviderTurn>,
    turn_sequence: u64,
}

impl AppController {
    pub fn new() -> Result<Self> {
        let config = AppConfig::from_env();
        let auth = AuthRuntime::bootstrap_with_config(&config)
            .context("failed to bootstrap auth runtime")?;
        Ok(Self::new_with_auth_and_config(auth, config))
    }

    #[cfg(test)]
    pub fn new_for_tests() -> Self {
        let mut config = AppConfig::default();
        config.openai.base_url = "mock://openai".to_string();
        Self::new_with_auth_and_config(AuthRuntime::for_tests(), config)
    }

    fn new_with_auth_and_config(auth: AuthRuntime, config: AppConfig) -> Self {
        let mut state = AppState::default();
        state.set_model_and_variant(config.model.clone(), config.openai.variant.clone());
        state.set_session_id(format!("session-{}", unix_timestamp_secs()));
        state.push_message(
            MessageRole::System,
            format!("Auth bootstrap: {}", auth.status_report()),
        );

        if let Some(warning) = auth.startup_warning() {
            state.push_message(MessageRole::System, format!("Auth warning: {warning}"));
            state.set_status(format!("Auth warning: {warning}"));
        } else {
            state.set_status(format!(
                "Auth bootstrap completed ({}). Enter sends, /help lists commands.",
                auth.status().label()
            ));
        }

        let provider = build_provider(&config);
        let provider_endpoint = if config.openai.base_url.starts_with("mock://") {
            config.openai.base_url.clone()
        } else {
            "https://chatgpt.com/backend-api/codex/responses".to_string()
        };
        state.push_message(
            MessageRole::System,
            format!(
                "Provider bootstrap: {} (endpoint: {})",
                provider.id(),
                provider_endpoint
            ),
        );

        Self {
            state,
            events: EventSource::new(Duration::from_millis(100)),
            auth,
            provider,
            active_provider_turn: None,
            turn_sequence: 0,
        }
    }

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        while self.state.running {
            self.poll_provider_event();

            terminal
                .draw(|frame| ui::draw(frame, &self.state))
                .context("failed to draw terminal frame")?;

            let event = self
                .events
                .next_event()
                .context("failed to read terminal event")?;

            self.handle_event(event);
            self.poll_provider_event();
        }

        Ok(())
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => self.handle_key(key),
            AppEvent::Submit => self.handle_submit(),
            AppEvent::Command(command) => self.handle_command(command),
            AppEvent::Provider(event) => self.handle_provider_event(event),
            AppEvent::CancelActiveTurn => self.handle_cancel_turn_request(),
            AppEvent::Tick => self.state.touch_tick(),
            AppEvent::Resize(width, height) => {
                self.state.set_status(format!(
                    "Resized to {width}x{height}. Enter sends. Commands: {LOCAL_COMMANDS_INLINE_NO_QUIT}."
                ));
            }
            AppEvent::Quit => self.state.request_quit(),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if self.state.input_mode != InputMode::Editing {
            return;
        }

        match key.code {
            KeyCode::Backspace => {
                self.state.input.backspace();
            }
            KeyCode::Delete => {
                self.state.input.delete();
            }
            KeyCode::Left => {
                self.state.input.move_left();
            }
            KeyCode::Right => {
                self.state.input.move_right();
            }
            KeyCode::Home => {
                self.state.input.move_home();
            }
            KeyCode::End => {
                self.state.input.move_end();
            }
            KeyCode::Char(character)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.state.input.insert_char(character);
            }
            _ => {}
        }
    }

    fn handle_submit(&mut self) {
        let submitted = self.state.input.take_text();

        if submitted.trim().is_empty() {
            self.state.set_status("Input is empty.");
            return;
        }

        self.state
            .push_message(MessageRole::User, submitted.as_str());

        match parse_local_command(&submitted) {
            Some(Ok(command)) => self.handle_event(AppEvent::Command(command)),
            Some(Err(error)) => {
                self.state
                    .push_message(MessageRole::System, format!("Command error: {error}"));
                self.state
                    .set_status("Command failed. Use /help for available commands.");
            }
            None => self.start_provider_turn(),
        }
    }

    fn handle_command(&mut self, command: LocalCommand) {
        match command {
            LocalCommand::Help => {
                self.state
                    .push_message(MessageRole::System, local_help_message());
                self.state
                    .set_status("Displayed local help. No network calls were made.");
            }
            LocalCommand::Login(mode) => self.handle_login_command(mode),
            LocalCommand::Auth => self.handle_auth_command(),
            LocalCommand::Logout => self.handle_logout_command(),
            LocalCommand::Cancel => self.handle_cancel_turn_request(),
            LocalCommand::Quit => {
                self.state
                    .set_status("Received /quit command. Closing app...");
                self.handle_event(AppEvent::Quit);
            }
        }
    }

    fn handle_login_command(&mut self, mode: LoginCommandMode) {
        let method = match mode {
            LoginCommandMode::Browser => AuthLoginMethod::Browser,
            LoginCommandMode::Headless => AuthLoginMethod::Headless,
        };

        match self.auth.login(method) {
            Ok(status) => {
                info!(
                    status = status.label(),
                    method = method.label(),
                    "oauth login command completed"
                );
                self.state.push_message(
                    MessageRole::System,
                    format!(
                        "Login succeeded via {}. {}",
                        method.label(),
                        self.auth.status_report()
                    ),
                );
                self.state
                    .set_status(format!("Login completed ({}).", status.label()));
            }
            Err(error) => {
                error!(
                    ?error,
                    method = method.label(),
                    "oauth login command failed"
                );
                self.state
                    .push_message(MessageRole::System, format!("Login failed: {error}"));
                self.state.set_status("Login failed. See logs for details.");
            }
        }
    }

    fn handle_auth_command(&mut self) {
        let report = self.auth.status_report();
        self.state.push_message(MessageRole::System, report.clone());
        self.state.set_status(format!(
            "Displayed auth status ({})",
            self.auth.status().label()
        ));
    }

    fn handle_logout_command(&mut self) {
        match self.auth.logout() {
            Ok(status) => {
                info!(status = status.label(), "logout command completed");
                self.state.push_message(
                    MessageRole::System,
                    format!("Logout completed. {}", self.auth.status_report()),
                );
                self.state
                    .set_status(format!("Logout completed ({}).", status.label()));
            }
            Err(error) => {
                warn!(?error, "logout command failed");
                self.state
                    .push_message(MessageRole::System, format!("Logout failed: {error}"));
                self.state
                    .set_status("Logout failed. See logs for details.");
            }
        }
    }

    fn handle_cancel_turn_request(&mut self) {
        let Some(active_turn) = self.active_provider_turn.as_ref() else {
            self.state.set_status("No active provider turn to cancel.");
            return;
        };

        if active_turn.handle.cancel() {
            info!(turn_id = active_turn.handle.turn_id(), "cancel requested");
            self.state.mark_turn_cancelling();
            self.state
                .set_status("Cancel requested. Waiting for provider stream to stop...");
        } else {
            self.state
                .set_status("Cancel already requested for active turn.");
        }
    }

    fn start_provider_turn(&mut self) {
        if self.state.turn_state.is_active() {
            self.state.set_status(
                "A provider turn is already active. Wait for completion or use /cancel.",
            );
            return;
        }

        let request = self.build_provider_turn_request();
        let turn_start_result = block_on_future(self.provider.stream_turn(request));

        match turn_start_result {
            Ok(turn) => {
                info!(
                    provider = self.provider.id(),
                    turn_id = turn.turn_id,
                    "provider turn started"
                );
                self.state.begin_streaming_turn(turn.turn_id.clone());
                self.state.set_status(format!(
                    "Streaming response via {}. Use /cancel or Ctrl+X to stop.",
                    self.provider.id()
                ));
                self.active_provider_turn = Some(ActiveProviderTurn {
                    event_rx: turn.event_rx,
                    handle: turn.handle,
                });
            }
            Err(error) => {
                error!(
                    ?error,
                    provider = self.provider.id(),
                    "provider turn failed to start"
                );
                self.state.push_message(
                    MessageRole::System,
                    format!("Provider request failed to start: {error}"),
                );
                self.state
                    .set_status("Provider request failed to start. See logs for details.");
            }
        }
    }

    fn build_provider_turn_request(&mut self) -> ProviderTurnRequest {
        let access_token = match self.auth.resolve_access_token_for_request() {
            Ok(token) => token,
            Err(error) => {
                error!(?error, "auth refresh-before-request check failed");
                self.state.push_message(
                    MessageRole::System,
                    format!("Auth refresh-before-request failed: {error}"),
                );
                None
            }
        };

        let turn_id = self.next_turn_id();
        let auth_context = ProviderAuthContext {
            access_token,
            source: self.auth.source().label().to_string(),
            status: self.auth.status().label().to_string(),
            provider_id: self.auth.provider_id().to_string(),
            account_id: self.auth.account_id_for_request(),
        };

        ProviderTurnRequest {
            turn_id,
            session_id: self.state.session_id.clone(),
            model: self.state.model.clone(),
            variant: self.state.variant.clone(),
            messages: self.build_provider_messages(),
            auth: auth_context,
        }
    }

    fn build_provider_messages(&self) -> Vec<ProviderMessage> {
        self.state
            .messages
            .iter()
            .filter(|message| message.role != MessageRole::System)
            .enumerate()
            .map(|(index, message)| ProviderMessage {
                id: format!("{}-{index}", self.state.session_id),
                role: message.role,
                content: message.text.clone(),
                timestamp: message.timestamp,
            })
            .collect()
    }

    fn handle_provider_event(&mut self, event: ProviderEvent) {
        match event {
            ProviderEvent::TextDelta(delta) => {
                if !self.state.append_assistant_delta(&delta) {
                    warn!("received provider delta without an active assistant message");
                    return;
                }

                if self.state.turn_state != TurnState::Cancelling {
                    self.state
                        .set_status("Streaming response... Use /cancel or Ctrl+X to stop.");
                }
            }
            ProviderEvent::Completed => {
                let turn_id = self
                    .state
                    .complete_active_turn()
                    .unwrap_or_else(|| "unknown-turn".to_string());
                self.active_provider_turn = None;
                self.state
                    .set_status(format!("Turn {turn_id} completed successfully."));
            }
            ProviderEvent::Cancelled => {
                let turn_id = self
                    .state
                    .cancel_active_turn()
                    .unwrap_or_else(|| "unknown-turn".to_string());
                self.active_provider_turn = None;
                self.state.set_status(format!("Turn {turn_id} cancelled."));
            }
            ProviderEvent::Error(message) => {
                error!(provider = self.provider.id(), error = %message, "provider stream error");
                self.state
                    .push_message(MessageRole::System, format!("Provider error: {message}"));
                let _ = self.state.fail_active_turn();
                self.active_provider_turn = None;
                self.state
                    .set_status("Provider error. See logs and system messages.");
            }
            ProviderEvent::ToolCallRequested(tool_call) => {
                warn!(
                    provider = self.provider.id(),
                    tool_name = tool_call.name,
                    "tool call requested before tool loop is implemented"
                );
                self.state.push_message(
                    MessageRole::System,
                    format!(
                        "Tool call requested (`{}`) but tool execution is not enabled in Phase 4.",
                        tool_call.name
                    ),
                );
                self.state
                    .set_status("Tool call requested, but tools are not enabled in this phase.");
            }
        }
    }

    fn poll_provider_event(&mut self) {
        match self.poll_provider_event_inner() {
            ProviderPollResult::Event(event) => self.handle_event(AppEvent::Provider(event)),
            ProviderPollResult::Disconnected => {
                warn!("provider stream disconnected unexpectedly");
                if self.state.has_active_turn() {
                    self.state.push_message(
                        MessageRole::System,
                        "Provider stream disconnected before completion.",
                    );
                    let _ = self.state.fail_active_turn();
                    self.state
                        .set_status("Provider stream disconnected unexpectedly.");
                }
                self.active_provider_turn = None;
            }
            ProviderPollResult::Empty => {}
        }
    }

    fn poll_provider_event_inner(&mut self) -> ProviderPollResult {
        let Some(active_turn) = self.active_provider_turn.as_mut() else {
            return ProviderPollResult::Empty;
        };

        match active_turn.event_rx.try_recv() {
            Ok(event) => ProviderPollResult::Event(event),
            Err(TryRecvError::Disconnected) => ProviderPollResult::Disconnected,
            Err(TryRecvError::Empty) => ProviderPollResult::Empty,
        }
    }

    fn next_turn_id(&mut self) -> String {
        self.turn_sequence += 1;
        format!("{}-turn-{}", self.state.session_id, self.turn_sequence)
    }
}

fn build_provider(config: &AppConfig) -> Box<dyn ModelProvider> {
    match config.provider.trim().to_ascii_lowercase().as_str() {
        "openai" | "openai-responses" => {
            Box::new(OpenAiResponsesProvider::new(config.openai.base_url.clone()))
        }
        unknown => {
            warn!(
                provider = unknown,
                "unsupported provider requested; falling back to openai-responses"
            );
            Box::new(OpenAiResponsesProvider::new(config.openai.base_url.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::provider::AuthStatus;

    fn wait_for_stream_to_finish(controller: &mut AppController) {
        for _ in 0..2_000 {
            controller.poll_provider_event();
            if !controller.state.has_active_turn() {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        panic!("provider stream did not finish in test timeout");
    }

    #[test]
    fn resize_event_updates_status_line() {
        let mut controller = AppController::new_for_tests();
        controller.handle_event(AppEvent::Resize(120, 42));

        assert!(
            controller.state.status_line.contains("120x42"),
            "status line should include new terminal size"
        );
    }

    #[test]
    fn quit_event_stops_controller_loop() {
        let mut controller = AppController::new_for_tests();
        controller.handle_event(AppEvent::Quit);

        assert!(!controller.state.running);
    }

    #[test]
    fn submit_adds_user_and_streamed_assistant_messages() {
        let mut controller = AppController::new_for_tests();
        controller.state.input.text = "/login".to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);

        let initial_len = controller.state.messages.len();
        controller.state.input.text = "hello from streaming test".to_string();
        controller.state.input.cursor = controller.state.input.text.len();

        controller.handle_event(AppEvent::Submit);

        assert_eq!(controller.state.messages.len(), initial_len + 2);
        assert_eq!(
            controller.state.messages[initial_len].role,
            MessageRole::User
        );
        assert!(controller.state.has_active_turn());
        assert_eq!(controller.state.turn_state, TurnState::Streaming);

        wait_for_stream_to_finish(&mut controller);

        assert_eq!(controller.state.turn_state, TurnState::Idle);
        assert_eq!(
            controller
                .state
                .messages
                .last()
                .expect("assistant message should exist")
                .role,
            MessageRole::Assistant
        );
        assert!(
            controller
                .state
                .messages
                .last()
                .expect("assistant message should exist")
                .text
                .contains("OpenAI streaming provider active")
        );
    }

    #[test]
    fn provider_request_filters_out_system_messages() {
        let mut controller = AppController::new_for_tests();
        controller.state.messages.clear();
        controller
            .state
            .push_message(MessageRole::System, "system bootstrap");
        controller.state.push_message(MessageRole::User, "hello");
        controller
            .state
            .push_message(MessageRole::Assistant, "hi there");
        controller
            .state
            .push_message(MessageRole::System, "system warning");

        let provider_messages = controller.build_provider_messages();

        assert_eq!(provider_messages.len(), 2);
        assert_eq!(provider_messages[0].role, MessageRole::User);
        assert_eq!(provider_messages[0].content, "hello");
        assert_eq!(provider_messages[1].role, MessageRole::Assistant);
        assert_eq!(provider_messages[1].content, "hi there");
    }

    #[test]
    fn help_command_adds_system_help_message() {
        let mut controller = AppController::new_for_tests();
        let initial_len = controller.state.messages.len();
        controller.state.input.text = "/help".to_string();
        controller.state.input.cursor = controller.state.input.text.len();

        controller.handle_event(AppEvent::Submit);

        assert_eq!(controller.state.messages.len(), initial_len + 2);
        assert_eq!(
            controller.state.messages[initial_len].role,
            MessageRole::User
        );
        assert_eq!(
            controller.state.messages[initial_len + 1].role,
            MessageRole::System
        );
        assert!(
            controller.state.messages[initial_len + 1]
                .text
                .contains("/cancel")
        );
    }

    #[test]
    fn login_command_marks_runtime_as_signed_in() {
        let mut controller = AppController::new_for_tests();
        controller.state.input.text = "/login".to_string();
        controller.state.input.cursor = controller.state.input.text.len();

        controller.handle_event(AppEvent::Submit);

        assert_eq!(controller.auth.status(), AuthStatus::SignedIn);
        assert!(
            controller
                .state
                .messages
                .last()
                .expect("message should exist")
                .text
                .contains("Login succeeded")
        );
    }

    #[test]
    fn login_headless_command_uses_headless_auth_mode() {
        let mut controller = AppController::new_for_tests();
        controller.state.input.text = "/login headless".to_string();
        controller.state.input.cursor = controller.state.input.text.len();

        controller.handle_event(AppEvent::Submit);

        assert_eq!(controller.auth.status(), AuthStatus::SignedIn);
        assert!(
            controller
                .state
                .messages
                .last()
                .expect("message should exist")
                .text
                .contains("via headless")
        );
    }

    #[test]
    fn auth_command_prints_auth_report() {
        let mut controller = AppController::new_for_tests();
        controller.state.input.text = "/auth".to_string();
        controller.state.input.cursor = controller.state.input.text.len();

        controller.handle_event(AppEvent::Submit);

        assert!(
            controller
                .state
                .messages
                .last()
                .expect("message should exist")
                .text
                .contains("Auth report")
        );
    }

    #[test]
    fn logout_command_clears_oauth_session() {
        let mut controller = AppController::new_for_tests();
        controller.state.input.text = "/login".to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);

        controller.state.input.text = "/logout".to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);

        assert_eq!(controller.auth.status(), AuthStatus::SignedOut);
        assert!(
            controller
                .state
                .messages
                .last()
                .expect("message should exist")
                .text
                .contains("Logout completed")
        );
    }

    #[test]
    fn cancel_command_stops_active_turn() {
        let mut controller = AppController::new_for_tests();
        controller.state.input.text = "/login".to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);

        controller.state.input.text =
            "please generate a longer streaming response so cancellation has time to happen"
                .to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);

        assert!(controller.state.has_active_turn());

        controller.state.input.text = "/cancel".to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);
        wait_for_stream_to_finish(&mut controller);

        assert_eq!(controller.state.turn_state, TurnState::Cancelled);
        assert!(
            controller
                .state
                .status_line
                .to_ascii_lowercase()
                .contains("cancel")
        );
    }

    #[test]
    fn cancel_shortcut_routes_to_cancel_event() {
        let mut controller = AppController::new_for_tests();
        controller.state.input.text = "/login".to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);

        controller.state.input.text =
            "please generate a longer streaming response so shortcut cancellation can happen"
                .to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);

        assert!(controller.state.has_active_turn());

        controller.handle_event(AppEvent::CancelActiveTurn);
        wait_for_stream_to_finish(&mut controller);

        assert_eq!(controller.state.turn_state, TurnState::Cancelled);
    }

    #[test]
    fn quit_command_routes_to_quit_event() {
        let mut controller = AppController::new_for_tests();
        controller.state.input.text = "/quit".to_string();
        controller.state.input.cursor = controller.state.input.text.len();

        controller.handle_event(AppEvent::Submit);

        assert!(!controller.state.running);
    }

    #[test]
    fn unknown_command_shows_parse_failure_path() {
        let mut controller = AppController::new_for_tests();
        let initial_len = controller.state.messages.len();
        controller.state.input.text = "/does-not-exist".to_string();
        controller.state.input.cursor = controller.state.input.text.len();

        controller.handle_event(AppEvent::Submit);

        assert_eq!(controller.state.messages.len(), initial_len + 2);
        assert_eq!(
            controller.state.messages[initial_len + 1].role,
            MessageRole::System
        );
        assert!(
            controller.state.messages[initial_len + 1]
                .text
                .contains("Unknown command")
        );
        assert!(controller.state.running);
    }

    #[test]
    fn typing_slash_q_does_not_quit_without_submit() {
        let mut controller = AppController::new_for_tests();
        let initial_len = controller.state.messages.len();

        controller.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::Char('/'),
            KeyModifiers::NONE,
        )));
        controller.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        )));

        assert!(controller.state.running);
        assert_eq!(controller.state.input.text, "/q");
        assert_eq!(controller.state.messages.len(), initial_len);
    }

    #[test]
    fn submit_of_slash_q_is_unknown_command_not_quit() {
        let mut controller = AppController::new_for_tests();
        let initial_len = controller.state.messages.len();
        controller.state.input.text = "/q".to_string();
        controller.state.input.cursor = controller.state.input.text.len();

        controller.handle_event(AppEvent::Submit);

        assert!(controller.state.running);
        assert_eq!(controller.state.messages.len(), initial_len + 2);
        assert_eq!(
            controller.state.messages[initial_len].role,
            MessageRole::User
        );
        assert_eq!(
            controller.state.messages[initial_len + 1].role,
            MessageRole::System
        );
        assert!(
            controller.state.messages[initial_len + 1]
                .text
                .contains("Unknown command")
        );
    }
}
