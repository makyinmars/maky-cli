use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::state::{AppState, InputMode, MessageRole};

pub fn draw(frame: &mut Frame<'_>, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let header_text = format!(
        "{} | turn: {} | last tick {} ms ago",
        state.status_line,
        state.turn_state.label(),
        state.last_tick.elapsed().as_millis()
    );
    let header = Paragraph::new(header_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Maky CLI - Local Chat"),
    );
    frame.render_widget(header, chunks[0]);

    let history_lines = if state.messages.is_empty() {
        vec![Line::from(Span::styled(
            "No messages yet.",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        let mut lines = Vec::new();

        for message in &state.messages {
            let prefix = format!("[{} @ {}] ", message.role.label(), message.timestamp);
            let continuation = " ".repeat(prefix.chars().count());
            let mut message_lines = message.text.lines();

            if let Some(first_line) = message_lines.next() {
                lines.push(Line::from(vec![
                    Span::styled(
                        prefix.clone(),
                        Style::default()
                            .fg(role_color(message.role))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(first_line.to_string()),
                ]));

                for continuation_line in message_lines {
                    lines.push(Line::from(vec![
                        Span::raw(continuation.clone()),
                        Span::raw(continuation_line.to_string()),
                    ]));
                }
            } else {
                lines.push(Line::from(vec![
                    Span::styled(
                        prefix,
                        Style::default()
                            .fg(role_color(message.role))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(""),
                ]));
            }
        }

        lines
    };

    let visible_history_rows = chunks[1].height.saturating_sub(2) as usize;
    let history_scroll = history_lines.len().saturating_sub(visible_history_rows) as u16;

    let history = Paragraph::new(history_lines)
        .block(Block::default().borders(Borders::ALL).title("History"))
        .scroll((history_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(history, chunks[1]);

    let input = Paragraph::new(state.input.text.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Input (Enter sends, /help, /quit)"),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(input, chunks[2]);

    if state.input_mode == InputMode::Editing {
        let cursor_chars_before = state.input.text[..state.input.cursor].chars().count() as u16;
        let max_cursor_col = chunks[2].width.saturating_sub(3);
        let clamped_cursor = cursor_chars_before.min(max_cursor_col);

        frame.set_cursor_position((
            chunks[2].x.saturating_add(1).saturating_add(clamped_cursor),
            chunks[2].y.saturating_add(1),
        ));
    }
}

fn role_color(role: MessageRole) -> Color {
    match role {
        MessageRole::System => Color::Yellow,
        MessageRole::User => Color::Cyan,
        MessageRole::Assistant => Color::Green,
    }
}
