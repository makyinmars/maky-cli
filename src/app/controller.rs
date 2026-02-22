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
        event::{AppEvent, EventSource, ScrollDirection},
        state::{
            AppState, CANCEL_SHORTCUT_LABEL, ChatMessage, InputMode, LOCAL_COMMANDS_INLINE_NO_QUIT,
            LOGIN_COMMAND_USAGE, LocalCommand, LoginCommandMode, MessageRole, SESSION_STATE_FRESH,
            SESSION_STATE_RESTORED, TurnState, parse_local_command,
        },
        ui,
    },
    auth::{AuthRuntime, provider::AuthLoginMethod},
    model::types::{Message as ProviderMessage, ProviderEvent},
    providers::{
        openai_responses::OpenAiResponsesProvider,
        provider::{ModelProvider, ProviderAuthContext, ProviderTurnRequest, TurnHandle},
    },
    storage::{
        config::AppConfig,
        sessions::{SessionEvent, SessionRecord, SessionStore},
        sqlite_sessions::SqliteSessionStore,
    },
    util::{block_on_future, new_session_id},
};

use super::StartupOptions;

fn local_help_message() -> String {
    format!(
        "Local commands:\n/help - show this help\n{LOGIN_COMMAND_USAGE} - start OAuth login\n/auth - show auth status\n/logout - clear persisted OAuth session\n/new - start a fresh session\n/cancel - cancel the active provider turn\n/quit - exit the app\nShortcut: {CANCEL_SHORTCUT_LABEL} also cancels an active turn."
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
    session_store: Box<dyn SessionStore>,
    active_provider_turn: Option<ActiveProviderTurn>,
    turn_sequence: u64,
}

impl AppController {
    pub fn new(startup: StartupOptions) -> Result<Self> {
        let config = AppConfig::from_env();
        let auth = AuthRuntime::bootstrap_with_config(&config)
            .context("failed to bootstrap auth runtime")?;
        let session_store: Box<dyn SessionStore> =
            Box::new(SqliteSessionStore::new(config.session_db_path.clone()));
        Ok(Self::new_with_auth_and_config(
            auth,
            config,
            startup,
            session_store,
        ))
    }

    #[cfg(test)]
    pub fn new_for_tests() -> Self {
        let mut config = AppConfig::default();
        config.openai.base_url = "mock://openai".to_string();
        config.session_db_path = format!(".maky/test-sessions-{}.db", new_session_id());
        let session_store: Box<dyn SessionStore> =
            Box::new(SqliteSessionStore::new(config.session_db_path.clone()));
        Self::new_with_auth_and_config(
            AuthRuntime::for_tests(),
            config,
            StartupOptions::default(),
            session_store,
        )
    }

    #[cfg(test)]
    fn new_for_tests_with_store(store: Box<dyn SessionStore>, startup: StartupOptions) -> Self {
        let mut config = AppConfig::default();
        config.openai.base_url = "mock://openai".to_string();
        Self::new_with_auth_and_config(AuthRuntime::for_tests(), config, startup, store)
    }

    fn new_with_auth_and_config(
        auth: AuthRuntime,
        config: AppConfig,
        startup: StartupOptions,
        session_store: Box<dyn SessionStore>,
    ) -> Self {
        let mut state = AppState::default();
        state.set_model_and_variant(config.model.clone(), config.openai.variant.clone());
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

        let mut controller = Self {
            state,
            events: EventSource::new(Duration::from_millis(100)),
            auth,
            provider,
            session_store,
            active_provider_turn: None,
            turn_sequence: 0,
        };
        controller.apply_startup_policy(startup);
        controller
    }

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        while self.state.running {
            self.drain_provider_events();

            terminal
                .draw(|frame| ui::draw(frame, &mut self.state))
                .context("failed to draw terminal frame")?;

            let event = if self.state.has_active_turn() {
                self.events
                    .next_event_with_timeout(Duration::from_millis(16))
                    .context("failed to read terminal event")?
            } else {
                Some(
                    self.events
                        .next_event()
                        .context("failed to read terminal event")?,
                )
            };

            if let Some(event) = event {
                self.handle_event(event);
            }
            self.drain_provider_events();
        }

        Ok(())
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => self.handle_key(key),
            AppEvent::Scroll(direction) => self.handle_scroll(direction),
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
            KeyCode::Up => {
                self.state.scroll_history_up(1);
            }
            KeyCode::Down => {
                self.state.scroll_history_down(1);
            }
            KeyCode::PageUp => {
                self.state.scroll_history_page_up();
            }
            KeyCode::PageDown => {
                self.state.scroll_history_page_down();
            }
            KeyCode::Home if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.scroll_history_top();
            }
            KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.scroll_history_bottom();
            }
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

    fn handle_scroll(&mut self, direction: ScrollDirection) {
        if self.state.input_mode != InputMode::Editing {
            return;
        }

        match direction {
            ScrollDirection::Up => self.state.scroll_history_up(1),
            ScrollDirection::Down => self.state.scroll_history_down(1),
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
        if let Some(user_message) = self.state.messages.last().cloned() {
            let index = self.state.messages.len().saturating_sub(1);
            self.persist_chat_message(&user_message, index);
        }

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
            LocalCommand::New => self.handle_new_session_command(),
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

    fn handle_new_session_command(&mut self) {
        if self.state.turn_state.is_active() {
            self.state
                .set_status("Finish or cancel the active provider turn before running /new.");
            return;
        }

        let previous_session_id = self.state.session_id.clone();
        let next_session_id = new_session_id();

        self.state.reset_for_new_session(next_session_id.clone());
        self.state.set_session_state(SESSION_STATE_FRESH);
        self.turn_sequence = 0;
        self.active_provider_turn = None;
        self.state.set_status(format!(
            "Started a fresh session ({next_session_id}). Previous session: {previous_session_id}."
        ));
        self.append_session_event(SessionEvent::Status(format!(
            "Session created via /new (previous session: {previous_session_id})"
        )));
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
        self.append_session_event(SessionEvent::Provider(event.clone()));

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
                let assistant_snapshot = self.active_assistant_message_snapshot();
                let turn_id = self
                    .state
                    .complete_active_turn()
                    .unwrap_or_else(|| "unknown-turn".to_string());
                self.active_provider_turn = None;
                self.persist_assistant_snapshot(assistant_snapshot);
                self.state
                    .set_status(format!("Turn {turn_id} completed successfully."));
            }
            ProviderEvent::Cancelled => {
                let assistant_snapshot = self.active_assistant_message_snapshot();
                let turn_id = self
                    .state
                    .cancel_active_turn()
                    .unwrap_or_else(|| "unknown-turn".to_string());
                self.active_provider_turn = None;
                self.persist_assistant_snapshot(assistant_snapshot);
                self.state.set_status(format!("Turn {turn_id} cancelled."));
            }
            ProviderEvent::Error(message) => {
                let assistant_snapshot = self.active_assistant_message_snapshot();
                error!(provider = self.provider.id(), error = %message, "provider stream error");
                self.state
                    .push_message(MessageRole::System, format!("Provider error: {message}"));
                let _ = self.state.fail_active_turn();
                self.active_provider_turn = None;
                self.persist_assistant_snapshot(assistant_snapshot);
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

    fn drain_provider_events(&mut self) -> bool {
        let mut handled_events = false;

        loop {
            match self.poll_provider_event_inner() {
                ProviderPollResult::Event(event) => {
                    handled_events = true;
                    self.handle_event(AppEvent::Provider(event));
                }
                ProviderPollResult::Disconnected => {
                    handled_events = true;
                    warn!("provider stream disconnected unexpectedly");
                    if self.state.has_active_turn() {
                        let assistant_snapshot = self.active_assistant_message_snapshot();
                        self.state.push_message(
                            MessageRole::System,
                            "Provider stream disconnected before completion.",
                        );
                        let _ = self.state.fail_active_turn();
                        self.persist_assistant_snapshot(assistant_snapshot);
                        self.append_session_event(SessionEvent::Status(
                            "Provider stream disconnected before completion.".to_string(),
                        ));
                        self.state
                            .set_status("Provider stream disconnected unexpectedly.");
                    }
                    self.active_provider_turn = None;
                    break;
                }
                ProviderPollResult::Empty => break,
            }
        }

        handled_events
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

    fn apply_startup_policy(&mut self, startup: StartupOptions) {
        if startup.force_new_session {
            self.state.set_session_id(new_session_id());
            self.state.set_session_state(SESSION_STATE_FRESH);
            self.state
                .set_status("Started a fresh session (--new). Enter sends, /help lists commands.");
            return;
        }

        if let Some(requested_session_id) = startup.resume_session_id {
            match self.session_store.load_session(&requested_session_id) {
                Ok(Some(record)) => {
                    self.restore_from_record(record);
                    self.state.push_message(
                        MessageRole::System,
                        format!("Resumed requested session `{requested_session_id}`."),
                    );
                    self.state.set_status(format!(
                        "Resumed session {requested_session_id}. Enter sends, /new starts fresh."
                    ));
                }
                Ok(None) => {
                    self.state.set_session_id(new_session_id());
                    self.state.set_session_state(SESSION_STATE_FRESH);
                    self.state.push_message(
                        MessageRole::System,
                        format!(
                            "Requested session `{requested_session_id}` was not found. Started a fresh session."
                        ),
                    );
                    self.state.set_status(format!(
                        "Session `{requested_session_id}` not found. Started a fresh session."
                    ));
                }
                Err(error) => {
                    warn!(?error, "failed to load requested resume session");
                    self.state.set_session_id(new_session_id());
                    self.state.set_session_state(SESSION_STATE_FRESH);
                    self.state.push_message(
                        MessageRole::System,
                        "Session resume failed. Started a fresh session.",
                    );
                    self.state
                        .set_status("Session resume failed. Started a fresh session.");
                }
            }
            return;
        }

        match self.session_store.load_latest() {
            Ok(Some(record)) => {
                let restored_session_id = record.meta.session_id.clone();
                self.restore_from_record(record);
                self.state.push_message(
                    MessageRole::System,
                    format!("Restored latest session `{restored_session_id}`."),
                );
                self.state.set_status(format!(
                    "Restored latest session {restored_session_id}. Enter sends, /new starts fresh."
                ));
            }
            Ok(None) => {
                self.state.set_session_id(new_session_id());
                self.state.set_session_state(SESSION_STATE_FRESH);
            }
            Err(error) => {
                warn!(?error, "failed to load latest session");
                self.state.set_session_id(new_session_id());
                self.state.set_session_state(SESSION_STATE_FRESH);
                self.state.push_message(
                    MessageRole::System,
                    "Latest-session restore failed. Started a fresh session.",
                );
                self.state
                    .set_status("Latest-session restore failed. Started a fresh session.");
            }
        }
    }

    fn restore_from_record(&mut self, record: SessionRecord) {
        self.state.set_session_id(record.meta.session_id.clone());
        self.state.set_session_state(SESSION_STATE_RESTORED);
        if !record.meta.model.trim().is_empty() {
            self.state.model = record.meta.model.clone();
        }
        self.turn_sequence = 0;
        self.active_provider_turn = None;
        self.state.turn_state = TurnState::Idle;
        self.state.active_turn_id = None;
        self.state.active_assistant_message_index = None;
        self.state.input = Default::default();
        self.state.reset_history_scroll();
        self.state.messages.clear();

        for event in record.events {
            if let SessionEvent::Message(message) = event {
                self.state.messages.push(crate::app::state::ChatMessage {
                    role: message.role,
                    text: message.content,
                    timestamp: message.timestamp,
                });
            }
        }

        if self.state.messages.is_empty() {
            self.state.messages = AppState::default_messages();
        }
    }

    fn append_session_event(&self, event: SessionEvent) {
        if let Err(error) =
            self.session_store
                .append_event(&self.state.session_id, &self.state.model, &event)
        {
            warn!(
                ?error,
                session_id = self.state.session_id,
                "failed to append session event"
            );
        }
    }

    fn persist_chat_message(&self, message: &ChatMessage, index: usize) {
        let event = SessionEvent::Message(ProviderMessage {
            id: format!("{}-{index}", self.state.session_id),
            role: message.role,
            content: message.text.clone(),
            timestamp: message.timestamp,
        });
        self.append_session_event(event);
    }

    fn active_assistant_message_snapshot(&self) -> Option<(usize, ChatMessage)> {
        self.state
            .active_assistant_message_index
            .and_then(|message_index| {
                self.state
                    .messages
                    .get(message_index)
                    .cloned()
                    .map(|message| (message_index, message))
            })
    }

    fn persist_assistant_snapshot(&self, assistant_snapshot: Option<(usize, ChatMessage)>) {
        let Some((message_index, message)) = assistant_snapshot else {
            return;
        };
        if message.text.trim().is_empty() {
            return;
        }

        self.persist_chat_message(&message, message_index);
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
    use std::{
        fs,
        path::Path,
        sync::{Mutex, OnceLock},
    };

    use super::*;
    use crate::{
        app::ui,
        auth::{
            AuthRuntime, CredentialSource,
            provider::AuthStatus,
            token_store::{FileTokenStore, StoredToken, TokenStore},
        },
        model::types::MessageRole,
        storage::{config::AppConfig, sessions::SessionStore, sqlite_sessions::SqliteSessionStore},
    };
    use ratatui::{Terminal, backend::TestBackend};
    use tempfile::tempdir;

    const AUTH_TOKEN_FILE_ENV: &str = "MAKY_AUTH_TOKEN_FILE";
    const AUTH_TOKEN_STORE_ENV: &str = "MAKY_AUTH_TOKEN_STORE";

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            // SAFETY: test-only env mutation is serialized by `env_var_lock`.
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(previous) => {
                    // SAFETY: test-only env mutation is serialized by `env_var_lock`.
                    unsafe {
                        std::env::set_var(self.key, previous);
                    }
                }
                None => {
                    // SAFETY: test-only env mutation is serialized by `env_var_lock`.
                    unsafe {
                        std::env::remove_var(self.key);
                    }
                }
            }
        }
    }

    fn env_var_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn write_auth_token_file(path: &Path) {
        let store = FileTokenStore::new(path.to_path_buf());
        store
            .save(&StoredToken {
                provider_id: "chatgpt-oauth".to_string(),
                access_token: "test-access-token".to_string(),
                refresh_token: Some("test-refresh-token".to_string()),
                expires_at_unix_secs: Some(u64::MAX),
                id_token: None,
                account_id: Some("test-account".to_string()),
            })
            .expect("token file should be written");
    }

    fn controller_with_token_file_auth() -> (tempfile::TempDir, AppController) {
        let dir = tempdir().expect("tempdir should be created");
        let maky_dir = dir.path().join(".maky");
        fs::create_dir_all(&maky_dir).expect(".maky dir should be created");

        let token_file_path = maky_dir.join("auth_tokens.json");
        write_auth_token_file(&token_file_path);

        let mut config = AppConfig::default();
        config.auth.token_store = "file".to_string();
        config.openai.base_url = "mock://openai".to_string();
        config.session_db_path = maky_dir.join("sessions.db").display().to_string();

        let auth = {
            let _env_lock = env_var_lock().lock().expect("env lock should be available");
            let _token_file_guard = EnvVarGuard::set(
                AUTH_TOKEN_FILE_ENV,
                token_file_path
                    .to_str()
                    .expect("token file path should be valid utf-8"),
            );
            let _token_store_guard = EnvVarGuard::set(AUTH_TOKEN_STORE_ENV, "file");
            AuthRuntime::bootstrap_with_config(&config)
                .expect("auth runtime should bootstrap from token file")
        };

        assert_eq!(auth.source(), CredentialSource::OAuthSession);

        let store: Box<dyn SessionStore> =
            Box::new(SqliteSessionStore::new(config.session_db_path.clone()));
        let controller = AppController::new_with_auth_and_config(
            auth,
            config,
            StartupOptions {
                force_new_session: true,
                ..StartupOptions::default()
            },
            store,
        );
        (dir, controller)
    }

    fn wait_for_stream_to_finish(controller: &mut AppController) {
        for _ in 0..2_000 {
            controller.drain_provider_events();
            if !controller.state.has_active_turn() {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        panic!("provider stream did not finish in test timeout");
    }

    fn temp_store() -> (tempfile::TempDir, SqliteSessionStore) {
        let dir = tempdir().expect("tempdir should be created");
        let store = SqliteSessionStore::new(dir.path().join("sessions.db"));
        (dir, store)
    }

    fn draw_ui_frame(controller: &mut AppController, terminal: &mut Terminal<TestBackend>) {
        terminal
            .draw(|frame| ui::draw(frame, &mut controller.state))
            .expect("ui draw should succeed");
    }

    fn pump_stream_with_draw(controller: &mut AppController, terminal: &mut Terminal<TestBackend>) {
        for _ in 0..2_000 {
            controller.drain_provider_events();
            draw_ui_frame(controller, terminal);
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

    #[test]
    fn history_scroll_keys_adjust_scroll_state() {
        let mut controller = AppController::new_for_tests();
        controller.state.set_history_layout(50, 10);

        controller.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::Up,
            KeyModifiers::NONE,
        )));
        assert_eq!(controller.state.effective_history_scroll(), 39);

        controller.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::PageUp,
            KeyModifiers::NONE,
        )));
        assert_eq!(controller.state.effective_history_scroll(), 29);

        controller.handle_event(AppEvent::Key(KeyEvent::new(
            KeyCode::End,
            KeyModifiers::CONTROL,
        )));
        assert!(controller.state.is_history_scrolled_to_bottom());
    }

    #[test]
    fn history_scroll_mouse_wheel_adjusts_scroll_state() {
        let mut controller = AppController::new_for_tests();
        controller.state.set_history_layout(50, 10);

        controller.handle_event(AppEvent::Scroll(ScrollDirection::Up));
        assert_eq!(controller.state.effective_history_scroll(), 39);

        controller.handle_event(AppEvent::Scroll(ScrollDirection::Down));
        assert!(controller.state.is_history_scrolled_to_bottom());
    }

    #[test]
    fn token_file_bootstrap_streaming_keeps_history_following_bottom() {
        let (_dir, mut controller) = controller_with_token_file_auth();
        let backend = TestBackend::new(24, 10);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

        controller.state.input.text = "stream enough tokens so the history viewport must keep moving while assistant output grows".to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);
        assert!(controller.state.has_active_turn());

        let mut saw_scroll_growth = false;
        let mut previous_scroll = controller.state.effective_history_scroll();

        for _ in 0..2_000 {
            controller.drain_provider_events();
            draw_ui_frame(&mut controller, &mut terminal);

            let current_scroll = controller.state.effective_history_scroll();
            if current_scroll > previous_scroll {
                saw_scroll_growth = true;
            }
            previous_scroll = current_scroll;

            if !controller.state.has_active_turn() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        assert!(
            !controller.state.has_active_turn(),
            "stream should complete within timeout"
        );
        assert!(
            saw_scroll_growth,
            "history scroll should advance as streaming content grows"
        );
        assert!(
            controller.state.is_history_scrolled_to_bottom(),
            "history should still follow bottom when user did not scroll away"
        );

        let assistant = controller
            .state
            .messages
            .last()
            .expect("assistant message should exist");
        assert_eq!(assistant.role, MessageRole::Assistant);
        assert!(
            assistant.text.contains("OpenAI streaming provider active"),
            "streamed response should be present"
        );
    }

    #[test]
    fn token_file_bootstrap_streaming_respects_manual_scroll_and_recovers_with_wheel_down() {
        let (_dir, mut controller) = controller_with_token_file_auth();
        let backend = TestBackend::new(24, 10);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

        controller.state.input.text =
            "stream a long wrapped response so manual scroll can pin history away from bottom"
                .to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);
        assert!(controller.state.has_active_turn());

        let mut reached_scrollable_state = false;
        for _ in 0..600 {
            controller.drain_provider_events();
            draw_ui_frame(&mut controller, &mut terminal);
            if controller.state.effective_history_scroll() > 0 {
                reached_scrollable_state = true;
                break;
            }
            if !controller.state.has_active_turn() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            reached_scrollable_state,
            "history should become scrollable during streaming"
        );

        controller.handle_event(AppEvent::Scroll(ScrollDirection::Up));
        draw_ui_frame(&mut controller, &mut terminal);
        let pinned_scroll = controller.state.effective_history_scroll();
        assert!(
            !controller.state.is_history_scrolled_to_bottom(),
            "wheel up should move history off bottom"
        );

        let assistant_len_before = controller
            .state
            .messages
            .last()
            .expect("assistant message should exist")
            .text
            .len();
        for _ in 0..120 {
            controller.drain_provider_events();
            draw_ui_frame(&mut controller, &mut terminal);
            if !controller.state.has_active_turn() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let assistant_len_after = controller
            .state
            .messages
            .last()
            .expect("assistant message should exist")
            .text
            .len();
        assert!(
            assistant_len_after > assistant_len_before,
            "assistant text should continue streaming while scrolled away from bottom"
        );
        assert_eq!(
            controller.state.effective_history_scroll(),
            pinned_scroll,
            "manual scroll position should stay pinned while streaming continues"
        );

        for _ in 0..128 {
            controller.handle_event(AppEvent::Scroll(ScrollDirection::Down));
            draw_ui_frame(&mut controller, &mut terminal);
            if controller.state.is_history_scrolled_to_bottom() {
                break;
            }
        }
        assert!(
            controller.state.is_history_scrolled_to_bottom(),
            "wheel down should return history to follow-bottom mode"
        );

        pump_stream_with_draw(&mut controller, &mut terminal);
    }

    #[test]
    fn startup_restores_latest_session_by_default() {
        let (_dir, store) = temp_store();
        let older_event = SessionEvent::Message(ProviderMessage {
            id: "old-user".to_string(),
            role: MessageRole::User,
            content: "older session prompt".to_string(),
            timestamp: 1_735_000_000,
        });
        let latest_event = SessionEvent::Message(ProviderMessage {
            id: "new-user".to_string(),
            role: MessageRole::User,
            content: "latest session prompt".to_string(),
            timestamp: 1_735_000_001,
        });

        store
            .append_event("session-older", "openai/gpt-5.3-codex", &older_event)
            .expect("older append should succeed");
        std::thread::sleep(Duration::from_secs(1));
        store
            .append_event("session-latest", "openai/gpt-5.3-codex", &latest_event)
            .expect("latest append should succeed");

        let controller = AppController::new_for_tests_with_store(
            Box::new(store.clone()),
            StartupOptions::default(),
        );

        assert_eq!(controller.state.session_id, "session-latest");
        assert_eq!(controller.state.session_state, SESSION_STATE_RESTORED);
        assert!(
            controller
                .state
                .messages
                .iter()
                .any(|message| message.text.contains("latest session prompt"))
        );
    }

    #[test]
    fn startup_can_resume_explicit_session_id() {
        let (_dir, store) = temp_store();
        let explicit_event = SessionEvent::Message(ProviderMessage {
            id: "resume-user".to_string(),
            role: MessageRole::User,
            content: "resume this conversation".to_string(),
            timestamp: 1_735_000_002,
        });

        store
            .append_event("session-resume-me", "openai/gpt-5.3-codex", &explicit_event)
            .expect("append should succeed");

        let controller = AppController::new_for_tests_with_store(
            Box::new(store),
            StartupOptions {
                resume_session_id: Some("session-resume-me".to_string()),
                force_new_session: false,
            },
        );

        assert_eq!(controller.state.session_id, "session-resume-me");
        assert_eq!(controller.state.session_state, SESSION_STATE_RESTORED);
        assert!(
            controller
                .state
                .messages
                .iter()
                .any(|message| message.text.contains("resume this conversation"))
        );
    }

    #[test]
    fn new_command_resets_history_and_rotates_session_id() {
        let mut controller = AppController::new_for_tests();
        let previous_session_id = controller.state.session_id.clone();

        controller.state.input.text = "/help".to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);
        assert!(controller.state.messages.len() > AppState::default_messages().len());

        controller.state.input.text = "/new".to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);

        assert_ne!(controller.state.session_id, previous_session_id);
        assert_eq!(controller.state.session_state, SESSION_STATE_FRESH);
        assert_eq!(controller.state.turn_state, TurnState::Idle);
        assert_eq!(
            controller.state.messages.len(),
            AppState::default_messages().len()
        );
    }

    #[test]
    fn provider_pipeline_persists_messages_and_stream_events() {
        let (_dir, store) = temp_store();
        let mut controller = AppController::new_for_tests_with_store(
            Box::new(store.clone()),
            StartupOptions {
                force_new_session: true,
                ..StartupOptions::default()
            },
        );

        controller.state.input.text = "/login".to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);

        controller.state.input.text = "persist this turn please".to_string();
        controller.state.input.cursor = controller.state.input.text.len();
        controller.handle_event(AppEvent::Submit);
        wait_for_stream_to_finish(&mut controller);

        let record = store
            .load_session(&controller.state.session_id)
            .expect("load should succeed")
            .expect("session record should exist");

        assert!(record.events.iter().any(|event| matches!(
            event,
            SessionEvent::Message(message)
                if message.role == MessageRole::User
                    && message.content.contains("persist this turn please")
        )));
        assert!(
            record
                .events
                .iter()
                .any(|event| matches!(event, SessionEvent::Provider(ProviderEvent::TextDelta(_))))
        );
        assert!(record.events.iter().any(|event| matches!(
            event,
            SessionEvent::Message(message)
                if message.role == MessageRole::Assistant
                    && message.content.contains("OpenAI streaming provider active")
        )));
    }
}
