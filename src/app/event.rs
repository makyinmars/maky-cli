use std::{
    io,
    time::{Duration, Instant},
};

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};

use crate::{app::state::LocalCommand, model::types::ProviderEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    Key(KeyEvent),
    Scroll(ScrollDirection),
    Submit,
    Command(LocalCommand),
    Provider(ProviderEvent),
    CancelActiveTurn,
    Tick,
    Resize(u16, u16),
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
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
            if let Some(event) = self.next_event_with_timeout(self.tick_rate)? {
                return Ok(event);
            }
        }
    }

    pub fn next_event_with_timeout(&mut self, max_wait: Duration) -> io::Result<Option<AppEvent>> {
        let start = Instant::now();

        loop {
            let tick_timeout = self.tick_rate.saturating_sub(self.last_tick.elapsed());
            let elapsed = start.elapsed();
            let remaining = max_wait.saturating_sub(elapsed);
            let timeout = tick_timeout.min(remaining);

            if event::poll(timeout)? {
                if let Some(event) = map_crossterm_event(event::read()?) {
                    return Ok(Some(event));
                }
                continue;
            }

            if self.last_tick.elapsed() >= self.tick_rate {
                self.last_tick = Instant::now();
                return Ok(Some(AppEvent::Tick));
            }

            if elapsed >= max_wait {
                return Ok(None);
            }
        }
    }
}

fn scroll_direction_from_mouse_kind(kind: MouseEventKind) -> Option<ScrollDirection> {
    match kind {
        MouseEventKind::ScrollUp => Some(ScrollDirection::Up),
        MouseEventKind::ScrollDown => Some(ScrollDirection::Down),
        _ => None,
    }
}

fn map_crossterm_event(event: Event) -> Option<AppEvent> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            if is_quit_key(&key) {
                return Some(AppEvent::Quit);
            }
            if is_cancel_key(&key) {
                return Some(AppEvent::CancelActiveTurn);
            }
            if is_submit_key(&key) {
                return Some(AppEvent::Submit);
            }
            Some(AppEvent::Key(key))
        }
        Event::Mouse(mouse) => scroll_direction_from_mouse_kind(mouse.kind).map(AppEvent::Scroll),
        Event::Resize(width, height) => Some(AppEvent::Resize(width, height)),
        _ => None,
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

    #[test]
    fn mouse_scroll_up_maps_to_scroll_direction_up() {
        assert_eq!(
            scroll_direction_from_mouse_kind(MouseEventKind::ScrollUp),
            Some(ScrollDirection::Up)
        );
    }

    #[test]
    fn mouse_moved_does_not_map_to_scroll_direction() {
        assert_eq!(
            scroll_direction_from_mouse_kind(MouseEventKind::Moved),
            None
        );
    }
}
