use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

impl MessageRole {
    pub fn label(&self) -> &'static str {
        match self {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalCommand {
    Help,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandParseError {
    UnknownCommand(String),
    UnexpectedArguments { command: &'static str, args: String },
}

impl std::fmt::Display for CommandParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandParseError::UnknownCommand(command) => {
                write!(f, "Unknown command `{command}`. Try /help.")
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
    HandlingLocalResponse,
}

impl TurnState {
    pub fn label(&self) -> &'static str {
        match self {
            TurnState::Idle => "idle",
            TurnState::HandlingLocalResponse => "local-response",
        }
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
    pub input_mode: InputMode,
    pub input: InputState,
    pub turn_state: TurnState,
    pub messages: Vec<ChatMessage>,
    pub last_tick: Instant,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            running: true,
            status_line: "Local mode active. Enter sends, /help shows commands, /quit exits."
                .to_string(),
            input_mode: InputMode::Editing,
            input: InputState::default(),
            turn_state: TurnState::Idle,
            messages: vec![
                ChatMessage::new(
                    MessageRole::System,
                    "Phase 2 local chat loop is active. No network/provider calls are made.",
                ),
                ChatMessage::new(
                    MessageRole::Assistant,
                    "Try typing a message, then Enter. Use /help for local commands.",
                ),
            ],
            last_tick: Instant::now(),
        }
    }
}

impl AppState {
    pub fn request_quit(&mut self) {
        self.running = false;
        self.status_line = "Shutting down...".to_string();
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status_line = status.into();
    }

    pub fn push_message(&mut self, role: MessageRole, text: impl Into<String>) {
        self.messages.push(ChatMessage::new(role, text));
    }

    pub fn touch_tick(&mut self) {
        self.last_tick = Instant::now();
    }
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_local_command() {
        assert_eq!(parse_local_command("/help"), Some(Ok(LocalCommand::Help)));
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
}
