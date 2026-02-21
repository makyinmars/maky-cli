use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::Backend};

use crate::app::{
    event::{AppEvent, EventSource},
    state::{AppState, InputMode, LocalCommand, MessageRole, TurnState, parse_local_command},
    ui,
};

const LOCAL_HELP_MESSAGE: &str = "Local commands:\n/help - show this help\n/quit - exit the app";

pub struct AppController {
    state: AppState,
    events: EventSource,
}

impl AppController {
    pub fn new() -> Self {
        Self {
            state: AppState::default(),
            events: EventSource::new(Duration::from_millis(250)),
        }
    }

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        while self.state.running {
            terminal
                .draw(|frame| ui::draw(frame, &self.state))
                .context("failed to draw terminal frame")?;

            let event = self
                .events
                .next_event()
                .context("failed to read terminal event")?;

            self.handle_event(event);
        }

        Ok(())
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => self.handle_key(key),
            AppEvent::Submit => self.handle_submit(),
            AppEvent::Command(command) => self.handle_command(command),
            AppEvent::Tick => self.state.touch_tick(),
            AppEvent::Resize(width, height) => {
                self.state.set_status(format!(
                    "Resized to {width}x{height}. Enter sends, /help lists commands, /quit exits."
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
            None => self.handle_local_response(submitted),
        }
    }

    fn handle_command(&mut self, command: LocalCommand) {
        match command {
            LocalCommand::Help => {
                self.state
                    .push_message(MessageRole::System, LOCAL_HELP_MESSAGE);
                self.state
                    .set_status("Displayed local help. No network calls were made.");
            }
            LocalCommand::Quit => {
                self.state
                    .set_status("Received /quit command. Closing app...");
                self.handle_event(AppEvent::Quit);
            }
        }
    }

    fn handle_local_response(&mut self, user_input: String) {
        self.state.turn_state = TurnState::HandlingLocalResponse;

        let response = fake_local_assistant_response(&user_input);

        self.state.push_message(MessageRole::Assistant, response);
        self.state.turn_state = TurnState::Idle;
        self.state
            .set_status("Local assistant response generated. Provider networking is disabled.");
    }
}

fn fake_local_assistant_response(user_input: &str) -> String {
    let word_count = user_input.split_whitespace().count();
    format!("Local assistant (deterministic): I received {word_count} word(s): {user_input}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_event_updates_status_line() {
        let mut controller = AppController::new();
        controller.handle_event(AppEvent::Resize(120, 42));

        assert!(
            controller.state.status_line.contains("120x42"),
            "status line should include new terminal size"
        );
    }

    #[test]
    fn quit_event_stops_controller_loop() {
        let mut controller = AppController::new();
        controller.handle_event(AppEvent::Quit);

        assert!(!controller.state.running);
    }

    #[test]
    fn submit_adds_user_and_local_assistant_messages() {
        let mut controller = AppController::new();
        let initial_len = controller.state.messages.len();
        controller.state.input.text = "hello from test".to_string();
        controller.state.input.cursor = controller.state.input.text.len();

        controller.handle_event(AppEvent::Submit);

        assert_eq!(controller.state.messages.len(), initial_len + 2);
        assert_eq!(
            controller.state.messages[initial_len].role,
            MessageRole::User
        );
        assert_eq!(
            controller.state.messages[initial_len + 1].role,
            MessageRole::Assistant
        );
        assert_eq!(controller.state.turn_state, TurnState::Idle);
    }

    #[test]
    fn help_command_adds_system_help_message() {
        let mut controller = AppController::new();
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
                .contains("/quit")
        );
    }

    #[test]
    fn quit_command_routes_to_quit_event() {
        let mut controller = AppController::new();
        controller.state.input.text = "/quit".to_string();
        controller.state.input.cursor = controller.state.input.text.len();

        controller.handle_event(AppEvent::Submit);

        assert!(!controller.state.running);
    }

    #[test]
    fn unknown_command_shows_parse_failure_path() {
        let mut controller = AppController::new();
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
        let mut controller = AppController::new();
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
        let mut controller = AppController::new();
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
