use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::state::AppState;

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
        "{} | last tick {} ms ago",
        state.status_line,
        state.last_tick.elapsed().as_millis()
    );
    let header =
        Paragraph::new(header_text).block(Block::default().borders(Borders::ALL).title("Header"));
    frame.render_widget(header, chunks[0]);

    let history_lines = if state.messages.is_empty() {
        vec![Line::from(Span::styled(
            "No messages yet.",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        state
            .messages
            .iter()
            .map(|line| {
                Line::from(vec![
                    Span::styled(
                        format!("[{}] ", line.role.label()),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(line.text.as_str()),
                ])
            })
            .collect()
    };

    let history = Paragraph::new(history_lines)
        .block(Block::default().borders(Borders::ALL).title("History"))
        .wrap(Wrap { trim: false });
    frame.render_widget(history, chunks[1]);

    let input_line = if state.input_buffer.is_empty() {
        Line::from(Span::styled(
            "Type here (placeholder input). Enter stores a local line.",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(state.input_buffer.as_str())
    };
    let input = Paragraph::new(input_line)
        .block(Block::default().borders(Borders::ALL).title("Input"))
        .wrap(Wrap { trim: false });
    frame.render_widget(input, chunks[2]);
}
