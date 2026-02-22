use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::state::{
    AppState, CANCEL_SHORTCUT_LABEL, ChatMessage, InputMode, LOCAL_COMMANDS_INLINE, MessageRole,
};

pub fn draw(frame: &mut Frame<'_>, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
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
            .title("Maky CLI - Streaming Chat"),
    );
    frame.render_widget(header, chunks[0]);

    let history_lines = build_history_lines(&state.messages);

    let visible_history_rows = chunks[1].height.saturating_sub(2) as usize;
    let history_scroll = history_lines.len().saturating_sub(visible_history_rows) as u16;

    let history = Paragraph::new(history_lines)
        .block(Block::default().borders(Borders::ALL).title("History"))
        .scroll((history_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(history, chunks[1]);

    let input_title =
        format!("Input (Enter sends, {LOCAL_COMMANDS_INLINE} | {CANCEL_SHORTCUT_LABEL} cancels)");
    let input = Paragraph::new(state.input.text.as_str())
        .block(Block::default().borders(Borders::ALL).title(input_title))
        .wrap(Wrap { trim: false });
    frame.render_widget(input, chunks[2]);

    let model_summary = Paragraph::new(build_model_variant_line(state));
    frame.render_widget(model_summary, chunks[3]);

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
        MessageRole::Tool => Color::Blue,
    }
}

fn build_model_variant_line(state: &AppState) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            "model: ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(state.model.clone(), Style::default().fg(Color::White)),
        Span::raw(" | "),
        Span::styled(
            "variant: ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(state.variant.clone(), Style::default().fg(Color::Cyan)),
    ])
}

fn build_history_lines(messages: &[ChatMessage]) -> Vec<Line<'static>> {
    if messages.is_empty() {
        return vec![Line::from(Span::styled(
            "No messages yet.",
            Style::default().fg(Color::DarkGray),
        ))];
    }

    let mut lines = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        append_message_lines(&mut lines, message);

        if index + 1 < messages.len() {
            lines.push(Line::default());
        }
    }

    lines
}

fn append_message_lines(lines: &mut Vec<Line<'static>>, message: &ChatMessage) {
    let prefix = role_prefix(message.role);
    let continuation = " ".repeat(prefix.chars().count());
    let role_style = role_prefix_style(message.role);
    let body_style = role_body_style(message.role);

    let mut message_lines = message.text.lines();
    if let Some(first_line) = message_lines.next() {
        lines.push(Line::from(vec![
            Span::styled(prefix.clone(), role_style),
            Span::styled(first_line.to_string(), body_style),
        ]));

        for continuation_line in message_lines {
            lines.push(Line::from(vec![
                Span::raw(continuation.clone()),
                Span::styled(continuation_line.to_string(), body_style),
            ]));
        }
    } else {
        lines.push(Line::from(Span::styled(prefix, role_style)));
    }
}

fn role_prefix(role: MessageRole) -> String {
    match role {
        MessageRole::System => "[system] ".to_string(),
        MessageRole::User => "> ".to_string(),
        MessageRole::Assistant => ". ".to_string(),
        MessageRole::Tool => "[tool] ".to_string(),
    }
}

fn role_prefix_style(role: MessageRole) -> Style {
    let mut style = Style::default().fg(role_color(role));
    if matches!(role, MessageRole::User | MessageRole::System) {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

fn role_body_style(role: MessageRole) -> Style {
    match role {
        MessageRole::System => Style::default().fg(Color::Gray),
        MessageRole::User => Style::default().fg(Color::White),
        MessageRole::Assistant => Style::default().fg(Color::Green),
        MessageRole::Tool => Style::default().fg(Color::Blue),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_lines_use_chat_style_prefixes() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::User,
                text: "hello".to_string(),
                timestamp: 1,
            },
            ChatMessage {
                role: MessageRole::Assistant,
                text: "hi".to_string(),
                timestamp: 2,
            },
            ChatMessage {
                role: MessageRole::System,
                text: "warning".to_string(),
                timestamp: 3,
            },
        ];

        let lines = build_history_lines(&messages);

        assert_eq!(line_text(&lines[0]), "> hello");
        assert_eq!(line_text(&lines[1]), "");
        assert_eq!(line_text(&lines[2]), ". hi");
        assert_eq!(line_text(&lines[3]), "");
        assert_eq!(line_text(&lines[4]), "[system] warning");
    }

    #[test]
    fn history_lines_indent_multiline_messages() {
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            text: "line one\nline two".to_string(),
            timestamp: 1,
        }];

        let lines = build_history_lines(&messages);

        assert_eq!(line_text(&lines[0]), ". line one");
        assert_eq!(line_text(&lines[1]), "  line two");
    }

    #[test]
    fn model_variant_line_shows_current_selection() {
        let state = AppState {
            model: "openai/gpt-5.3-codex".to_string(),
            variant: "xhigh".to_string(),
            ..AppState::default()
        };

        let line = build_model_variant_line(&state);

        assert_eq!(
            line_text(&line),
            "model: openai/gpt-5.3-codex | variant: xhigh"
        );
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }
}
