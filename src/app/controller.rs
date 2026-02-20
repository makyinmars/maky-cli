use std::{mem, time::Duration};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::Backend};

use crate::app::{
    event::{AppEvent, EventSource},
    state::{AppState, MessageLine, MessageRole},
    ui,
};

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
            AppEvent::Tick => self.state.touch_tick(),
            AppEvent::Resize(width, height) => {
                self.state.set_status(format!(
                    "Resized to {width}x{height}. Press q, Esc, or Ctrl+C to quit."
                ));
            }
            AppEvent::Quit => self.state.request_quit(),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Backspace => {
                self.state.input_buffer.pop();
            }
            KeyCode::Enter => {
                if self.state.input_buffer.trim().is_empty() {
                    self.state.set_status("Input is empty.");
                } else {
                    let submitted = mem::take(&mut self.state.input_buffer);
                    self.state
                        .messages
                        .push(MessageLine::new(MessageRole::User, submitted));
                    self.state.set_status(
                        "Stored input locally only. Provider/chat flow comes in later phases.",
                    );
                }
            }
            KeyCode::Char(character)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.state.input_buffer.push(character);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::event::AppEvent;

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
    fn enter_moves_input_into_message_history() {
        let mut controller = AppController::new();
        controller.state.input_buffer = "hello from test".to_string();
        let initial_len = controller.state.messages.len();

        controller.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(controller.state.input_buffer.is_empty());
        assert_eq!(controller.state.messages.len(), initial_len + 1);
        assert_eq!(
            controller
                .state
                .messages
                .last()
                .expect("a message should be pushed")
                .text,
            "hello from test"
        );
    }
}
