use std::time::Instant;

pub use crate::model::types::MessageRole;
use crate::model::types::ToolCall;
use crate::util::{new_session_id, unix_timestamp_secs};

pub const LOCAL_COMMANDS_INLINE: &str = "/help, /login [browser|headless], /auth, /logout, /new, /resume <id>, /cancel, /approve, /deny, /quit";
pub const LOCAL_COMMANDS_INLINE_NO_QUIT: &str = "/help, /login [browser|headless], /auth, /logout, /new, /resume <id>, /cancel, /approve, /deny";
pub const LOGIN_COMMAND_USAGE: &str = "/login [browser|headless]";
pub const CANCEL_SHORTCUT_LABEL: &str = "Ctrl+X";
pub const SESSION_STATE_FRESH: &str = "fresh";
pub const SESSION_STATE_RESTORED: &str = "restored";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub text: String,
    pub timestamp: u64,
}

impl ChatMessage {
    pub fn new(role: MessageRole, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
            timestamp: unix_timestamp_secs(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalCommand {
    Help,
    Login(LoginCommandMode),
    Auth,
    Logout,
    New,
    Resume(String),
    Cancel,
    Approve,
    Deny,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginCommandMode {
    Browser,
    Headless,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandParseError {
    UnknownCommand(String),
    MissingArguments {
        command: &'static str,
        usage: &'static str,
    },
    UnexpectedArguments {
        command: &'static str,
        args: String,
    },
}

impl std::fmt::Display for CommandParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandParseError::UnknownCommand(command) => {
                write!(f, "Unknown command `{command}`. Try /help.")
            }
            CommandParseError::MissingArguments { command, usage } => {
                write!(f, "Command `{command}` requires arguments. Usage: {usage}")
            }
            CommandParseError::UnexpectedArguments { command, args } => {
                write!(f, "Command `{command}` does not accept arguments: {args}")
            }
        }
    }
}

pub fn parse_local_command(input: &str) -> Option<Result<LocalCommand, CommandParseError>> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let mut parts = trimmed.split_whitespace();
    let command_name = parts.next().unwrap_or_default();
    let args = parts.collect::<Vec<_>>().join(" ");

    match command_name {
        "/help" => {
            if args.is_empty() {
                Some(Ok(LocalCommand::Help))
            } else {
                Some(Err(CommandParseError::UnexpectedArguments {
                    command: "/help",
                    args,
                }))
            }
        }
        "/login" => match args.as_str() {
            "" | "browser" => Some(Ok(LocalCommand::Login(LoginCommandMode::Browser))),
            "headless" => Some(Ok(LocalCommand::Login(LoginCommandMode::Headless))),
            _ => Some(Err(CommandParseError::UnexpectedArguments {
                command: "/login",
                args,
            })),
        },
        "/auth" => {
            if args.is_empty() {
                Some(Ok(LocalCommand::Auth))
            } else {
                Some(Err(CommandParseError::UnexpectedArguments {
                    command: "/auth",
                    args,
                }))
            }
        }
        "/logout" => {
            if args.is_empty() {
                Some(Ok(LocalCommand::Logout))
            } else {
                Some(Err(CommandParseError::UnexpectedArguments {
                    command: "/logout",
                    args,
                }))
            }
        }
        "/new" => {
            if args.is_empty() {
                Some(Ok(LocalCommand::New))
            } else {
                Some(Err(CommandParseError::UnexpectedArguments {
                    command: "/new",
                    args,
                }))
            }
        }
        "/resume" => {
            if args.is_empty() {
                Some(Err(CommandParseError::MissingArguments {
                    command: "/resume",
                    usage: "/resume <id>",
                }))
            } else {
                Some(Ok(LocalCommand::Resume(args)))
            }
        }
        "/cancel" => {
            if args.is_empty() {
                Some(Ok(LocalCommand::Cancel))
            } else {
                Some(Err(CommandParseError::UnexpectedArguments {
                    command: "/cancel",
                    args,
                }))
            }
        }
        "/approve" => {
            if args.is_empty() {
                Some(Ok(LocalCommand::Approve))
            } else {
                Some(Err(CommandParseError::UnexpectedArguments {
                    command: "/approve",
                    args,
                }))
            }
        }
        "/deny" => {
            if args.is_empty() {
                Some(Ok(LocalCommand::Deny))
            } else {
                Some(Err(CommandParseError::UnexpectedArguments {
                    command: "/deny",
                    args,
                }))
            }
        }
        "/quit" => {
            if args.is_empty() {
                Some(Ok(LocalCommand::Quit))
            } else {
                Some(Err(CommandParseError::UnexpectedArguments {
                    command: "/quit",
                    args,
                }))
            }
        }
        _ => Some(Err(CommandParseError::UnknownCommand(
            command_name.to_string(),
        ))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Editing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    Idle,
    Streaming,
    Cancelling,
    Cancelled,
}

impl TurnState {
    pub fn label(&self) -> &'static str {
        match self {
            TurnState::Idle => "idle",
            TurnState::Streaming => "streaming",
            TurnState::Cancelling => "cancelling",
            TurnState::Cancelled => "cancelled",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, TurnState::Streaming | TurnState::Cancelling)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputState {
    pub text: String,
    pub cursor: usize,
}

impl InputState {
    pub fn insert_char(&mut self, character: char) {
        self.text.insert(self.cursor, character);
        self.cursor += character.len_utf8();
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }

        let previous = self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0);

        self.text.drain(previous..self.cursor);
        self.cursor = previous;
        true
    }

    pub fn delete(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }

        let next = self.cursor
            + self.text[self.cursor..]
                .chars()
                .next()
                .expect("cursor points to a valid char boundary")
                .len_utf8();

        self.text.drain(self.cursor..next);
        true
    }

    pub fn move_left(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }

        self.cursor = self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0);
        true
    }

    pub fn move_right(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }

        self.cursor += self.text[self.cursor..]
            .chars()
            .next()
            .expect("cursor points to a valid char boundary")
            .len_utf8();
        true
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    pub fn take_text(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.text)
    }
}

#[derive(Debug)]
pub struct AppState {
    pub running: bool,
    pub status_line: String,
    pub model: String,
    pub variant: String,
    pub session_id: String,
    pub session_state: String,
    pub input_mode: InputMode,
    pub input: InputState,
    pub turn_state: TurnState,
    pub active_turn_id: Option<String>,
    pub active_assistant_message_index: Option<usize>,
    pub pending_tool_call: Option<ToolCall>,
    pub messages: Vec<ChatMessage>,
    pub history_scroll: usize,
    pub history_total_lines: usize,
    pub history_visible_rows: usize,
    pub last_tick: Instant,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            running: true,
            status_line: format!(
                "Provider mode active. Enter sends. Commands: {LOCAL_COMMANDS_INLINE}."
            ),
            model: "openai/gpt-5.3-codex".to_string(),
            variant: "medium".to_string(),
            session_id: new_session_id(),
            session_state: SESSION_STATE_FRESH.to_string(),
            input_mode: InputMode::Editing,
            input: InputState::default(),
            turn_state: TurnState::Idle,
            active_turn_id: None,
            active_assistant_message_index: None,
            pending_tool_call: None,
            messages: Self::default_messages(),
            history_scroll: usize::MAX,
            history_total_lines: 0,
            history_visible_rows: 0,
            last_tick: Instant::now(),
        }
    }
}

impl AppState {
    pub fn default_messages() -> Vec<ChatMessage> {
        vec![
            ChatMessage::new(
                MessageRole::System,
                "Phase 5 session architecture is active. Use /auth to inspect session status; run /login only when signed out.",
            ),
            ChatMessage::new(
                MessageRole::Assistant,
                "Try typing a prompt, then press Enter. Use /new for a fresh session, /resume <id> to restore one, or /cancel to stop an active stream.",
            ),
        ]
    }

    pub fn request_quit(&mut self) {
        self.running = false;
        self.status_line = "Shutting down...".to_string();
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status_line = status.into();
    }

    pub fn set_model_and_variant(&mut self, model: impl Into<String>, variant: impl Into<String>) {
        self.model = model.into();
        self.variant = variant.into();
    }

    pub fn set_session_id(&mut self, session_id: impl Into<String>) {
        self.session_id = session_id.into();
    }

    pub fn set_session_state(&mut self, session_state: impl Into<String>) {
        self.session_state = session_state.into();
    }

    pub fn push_message(&mut self, role: MessageRole, text: impl Into<String>) {
        let follow_bottom = self.is_history_scrolled_to_bottom();
        self.messages.push(ChatMessage::new(role, text));
        if follow_bottom {
            self.scroll_history_bottom();
        }
    }

    pub fn set_history_layout(&mut self, total_lines: usize, visible_rows: usize) {
        self.history_total_lines = total_lines;
        self.history_visible_rows = visible_rows;
        self.normalize_history_scroll();
    }

    pub fn effective_history_scroll(&self) -> usize {
        let max_scroll = self.max_history_scroll();
        if self.history_scroll == usize::MAX {
            max_scroll
        } else {
            self.history_scroll.min(max_scroll)
        }
    }

    pub fn scroll_history_up(&mut self, lines: usize) {
        let current = self.effective_history_scroll();
        self.history_scroll = current.saturating_sub(lines.max(1));
    }

    pub fn scroll_history_down(&mut self, lines: usize) {
        let max_scroll = self.max_history_scroll();
        let next = self
            .effective_history_scroll()
            .saturating_add(lines.max(1))
            .min(max_scroll);
        if next >= max_scroll {
            self.history_scroll = usize::MAX;
        } else {
            self.history_scroll = next;
        }
    }

    pub fn scroll_history_page_up(&mut self) {
        self.scroll_history_up(self.history_visible_rows.max(1));
    }

    pub fn scroll_history_page_down(&mut self) {
        self.scroll_history_down(self.history_visible_rows.max(1));
    }

    pub fn scroll_history_top(&mut self) {
        self.history_scroll = 0;
    }

    pub fn scroll_history_bottom(&mut self) {
        self.history_scroll = usize::MAX;
    }

    pub fn is_history_scrolled_to_bottom(&self) -> bool {
        let max_scroll = self.max_history_scroll();
        if max_scroll == 0 {
            return true;
        }
        self.history_scroll == usize::MAX || self.history_scroll >= max_scroll
    }

    pub fn reset_history_scroll(&mut self) {
        self.history_scroll = usize::MAX;
        self.history_total_lines = 0;
        self.history_visible_rows = 0;
    }

    pub fn reset_for_new_session(&mut self, session_id: impl Into<String>) {
        self.session_id = session_id.into();
        self.session_state = SESSION_STATE_FRESH.to_string();
        self.turn_state = TurnState::Idle;
        self.active_turn_id = None;
        self.active_assistant_message_index = None;
        self.input = InputState::default();
        self.messages = Self::default_messages();
        self.pending_tool_call = None;
        self.reset_history_scroll();
        self.last_tick = Instant::now();
    }

    pub fn begin_streaming_turn(&mut self, turn_id: impl Into<String>) {
        self.turn_state = TurnState::Streaming;
        self.pending_tool_call = None;
        self.active_turn_id = Some(turn_id.into());
        self.messages
            .push(ChatMessage::new(MessageRole::Assistant, String::new()));
        self.active_assistant_message_index = Some(self.messages.len() - 1);
        self.scroll_history_bottom();
    }

    pub fn has_active_turn(&self) -> bool {
        self.active_turn_id.is_some()
    }

    pub fn append_assistant_delta(&mut self, delta: &str) -> bool {
        let Some(message_index) = self.active_assistant_message_index else {
            return false;
        };

        if let Some(message) = self.messages.get_mut(message_index) {
            message.text.push_str(delta);
            true
        } else {
            false
        }
    }

    pub fn mark_turn_cancelling(&mut self) -> bool {
        if !self.has_active_turn() {
            return false;
        }

        self.turn_state = TurnState::Cancelling;
        true
    }

    pub fn complete_active_turn(&mut self) -> Option<String> {
        let turn_id = self.active_turn_id.take();
        self.active_assistant_message_index = None;
        self.turn_state = TurnState::Idle;
        turn_id
    }

    pub fn cancel_active_turn(&mut self) -> Option<String> {
        let turn_id = self.active_turn_id.take();
        self.active_assistant_message_index = None;
        self.turn_state = TurnState::Cancelled;
        self.drop_empty_active_assistant_message();
        turn_id
    }

    pub fn fail_active_turn(&mut self) -> Option<String> {
        let turn_id = self.active_turn_id.take();
        self.active_assistant_message_index = None;
        self.turn_state = TurnState::Idle;
        self.drop_empty_active_assistant_message();
        turn_id
    }

    fn drop_empty_active_assistant_message(&mut self) {
        let Some(last_message) = self.messages.last() else {
            return;
        };

        if last_message.role == MessageRole::Assistant && last_message.text.trim().is_empty() {
            self.messages.pop();
        }
    }

    pub fn touch_tick(&mut self) {
        self.last_tick = Instant::now();
    }

    fn max_history_scroll(&self) -> usize {
        self.history_total_lines
            .saturating_sub(self.history_visible_rows)
    }

    fn normalize_history_scroll(&mut self) {
        let max_scroll = self.max_history_scroll();
        if self.history_scroll == usize::MAX {
            return;
        }
        if self.history_scroll >= max_scroll {
            self.history_scroll = usize::MAX;
            return;
        }
        self.history_scroll = self.history_scroll.min(max_scroll);
    }

    pub fn set_pending_tool_call(&mut self, call: ToolCall) {
        self.pending_tool_call = Some(call);
    }

    #[cfg(test)]
    pub fn has_pending_tool_call(&self) -> bool {
        self.pending_tool_call.is_some()
    }

    pub fn take_pending_tool_call(&mut self) -> Option<ToolCall> {
        self.pending_tool_call.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_local_command() {
        assert_eq!(parse_local_command("/help"), Some(Ok(LocalCommand::Help)));
        assert_eq!(
            parse_local_command("/login"),
            Some(Ok(LocalCommand::Login(LoginCommandMode::Browser)))
        );
        assert_eq!(
            parse_local_command("/login browser"),
            Some(Ok(LocalCommand::Login(LoginCommandMode::Browser)))
        );
        assert_eq!(
            parse_local_command("/login headless"),
            Some(Ok(LocalCommand::Login(LoginCommandMode::Headless)))
        );
        assert_eq!(parse_local_command("/auth"), Some(Ok(LocalCommand::Auth)));
        assert_eq!(
            parse_local_command("/logout"),
            Some(Ok(LocalCommand::Logout))
        );
        assert_eq!(parse_local_command("/new"), Some(Ok(LocalCommand::New)));
        assert_eq!(
            parse_local_command("/resume session-123"),
            Some(Ok(LocalCommand::Resume("session-123".to_string())))
        );
        assert_eq!(
            parse_local_command("/cancel"),
            Some(Ok(LocalCommand::Cancel))
        );
        assert_eq!(
            parse_local_command("/approve"),
            Some(Ok(LocalCommand::Approve))
        );
        assert_eq!(parse_local_command("/deny"), Some(Ok(LocalCommand::Deny)));
    }

    #[test]
    fn resume_command_requires_session_id() {
        let result = parse_local_command("/resume").expect("slash command should be parsed");
        assert!(matches!(
            result,
            Err(CommandParseError::MissingArguments {
                command: "/resume",
                usage: "/resume <id>"
            })
        ));
    }

    #[test]
    fn command_with_arguments_is_rejected() {
        let result = parse_local_command("/quit now").expect("slash command should be parsed");
        assert!(matches!(
            result,
            Err(CommandParseError::UnexpectedArguments {
                command: "/quit",
                ..
            })
        ));
    }

    #[test]
    fn new_command_with_arguments_is_rejected() {
        let result = parse_local_command("/new now").expect("slash command should be parsed");
        assert!(matches!(
            result,
            Err(CommandParseError::UnexpectedArguments {
                command: "/new",
                ..
            })
        ));
    }

    #[test]
    fn unknown_local_command_is_rejected() {
        let result = parse_local_command("/unknown").expect("slash command should be parsed");
        assert!(matches!(result, Err(CommandParseError::UnknownCommand(_))));
    }

    #[test]
    fn non_command_input_is_not_parsed_as_command() {
        assert_eq!(parse_local_command("hello"), None);
    }

    #[test]
    fn input_state_supports_insert_and_cursor_moves() {
        let mut input = InputState::default();
        input.insert_char('h');
        input.insert_char('i');
        input.move_left();
        input.insert_char('!');

        assert_eq!(input.text, "h!i");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn backspace_removes_character_before_cursor() {
        let mut input = InputState {
            text: "rust".to_string(),
            cursor: 4,
        };

        assert!(input.backspace());
        assert_eq!(input.text, "rus");
        assert_eq!(input.cursor, 3);
    }

    #[test]
    fn history_scroll_defaults_to_follow_bottom() {
        let mut state = AppState::default();
        state.set_history_layout(20, 5);

        assert_eq!(state.effective_history_scroll(), 15);
        assert!(state.is_history_scrolled_to_bottom());
    }

    #[test]
    fn history_scroll_up_moves_off_bottom_and_down_repins() {
        let mut state = AppState::default();
        state.set_history_layout(30, 10);

        state.scroll_history_up(3);
        assert_eq!(state.effective_history_scroll(), 17);
        assert!(!state.is_history_scrolled_to_bottom());

        state.scroll_history_down(100);
        assert_eq!(state.effective_history_scroll(), 20);
        assert!(state.is_history_scrolled_to_bottom());
        assert_eq!(state.history_scroll, usize::MAX);
    }

    #[test]
    fn history_page_scrolling_uses_visible_rows() {
        let mut state = AppState::default();
        state.set_history_layout(80, 12);

        state.scroll_history_page_up();
        assert_eq!(state.effective_history_scroll(), 56);

        state.scroll_history_page_down();
        assert_eq!(state.effective_history_scroll(), 68);
        assert_eq!(state.history_scroll, usize::MAX);
    }

    #[test]
    fn begin_streaming_turn_forces_history_follow_bottom() {
        let mut state = AppState::default();
        state.set_history_layout(80, 12);
        state.scroll_history_top();
        assert!(!state.is_history_scrolled_to_bottom());

        state.begin_streaming_turn("turn-1");

        assert_eq!(state.history_scroll, usize::MAX);
        assert!(state.is_history_scrolled_to_bottom());
    }
}
