use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
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
pub struct MessageLine {
    pub role: MessageRole,
    pub text: String,
}

impl MessageLine {
    pub fn new(role: MessageRole, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
        }
    }
}

#[derive(Debug)]
pub struct AppState {
    pub running: bool,
    pub status_line: String,
    pub input_buffer: String,
    pub messages: Vec<MessageLine>,
    pub last_tick: Instant,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            running: true,
            status_line: "Press q, Esc, or Ctrl+C to quit.".to_string(),
            input_buffer: String::new(),
            messages: vec![
                MessageLine::new(
                    MessageRole::System,
                    "Terminal skeleton is live. Chat/provider logic comes in later phases.",
                ),
                MessageLine::new(
                    MessageRole::Assistant,
                    "UI loop is active. Press q, Esc, or Ctrl+C to quit.",
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

    pub fn touch_tick(&mut self) {
        self.last_tick = Instant::now();
    }
}
