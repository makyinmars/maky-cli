use std::{
    io,
    time::{Duration, Instant},
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{app::state::LocalCommand, model::types::ProviderEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    Key(KeyEvent),
    Submit,
    Command(LocalCommand),
    Provider(ProviderEvent),
    CancelActiveTurn,
    Tick,
    Resize(u16, u16),
    Quit,
}

pub struct EventSource {
    tick_rate: Duration,
    last_tick: Instant,
}

impl EventSource {
    pub fn new(tick_rate: Duration) -> Self {
        Self {
            tick_rate,
            last_tick: Instant::now(),
        }
    }

    pub fn next_event(&mut self) -> io::Result<AppEvent> {
        loop {
            let timeout = self.tick_rate.saturating_sub(self.last_tick.elapsed());

            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        if is_quit_key(&key) {
                            return Ok(AppEvent::Quit);
                        }
                        if is_cancel_key(&key) {
                            return Ok(AppEvent::CancelActiveTurn);
                        }
                        if is_submit_key(&key) {
                            return Ok(AppEvent::Submit);
                        }
                        return Ok(AppEvent::Key(key));
                    }
                    Event::Resize(width, height) => return Ok(AppEvent::Resize(width, height)),
                    _ => {}
                }
            }

            if self.last_tick.elapsed() >= self.tick_rate {
                self.last_tick = Instant::now();
                return Ok(AppEvent::Tick);
            }
        }
    }
}

pub fn is_submit_key(key: &KeyEvent) -> bool {
    key.code == KeyCode::Enter
}

pub fn is_quit_key(key: &KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => true,
        KeyCode::Char('c') | KeyCode::Char('C') => key.modifiers.contains(KeyModifiers::CONTROL),
        _ => false,
    }
}

pub fn is_cancel_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('x') | KeyCode::Char('X'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esc_is_quit_key() {
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert!(is_quit_key(&key));
    }

    #[test]
    fn ctrl_c_is_quit_key() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(is_quit_key(&key));
    }

    #[test]
    fn ctrl_q_is_not_quit_key() {
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert!(!is_quit_key(&key));
    }

    #[test]
    fn plain_q_is_not_quit_key() {
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(!is_quit_key(&key));
    }

    #[test]
    fn uppercase_q_is_not_quit_key() {
        let key = KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT);
        assert!(!is_quit_key(&key));
    }

    #[test]
    fn enter_is_submit_key() {
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert!(is_submit_key(&key));
    }

    #[test]
    fn ctrl_x_is_cancel_key() {
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL);
        assert!(is_cancel_key(&key));
    }

    #[test]
    fn plain_x_is_not_cancel_key() {
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert!(!is_cancel_key(&key));
    }
}
