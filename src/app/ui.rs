use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{
    markdown::render_markdown_lines,
    state::{
        AppState, CANCEL_SHORTCUT_LABEL, ChatMessage, InputMode, LOCAL_COMMANDS_INLINE, MessageRole,
    },
};

pub fn draw(frame: &mut Frame<'_>, state: &mut AppState) {
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
    let history_content_width = chunks[1].width.saturating_sub(2);
    let total_history_rows = total_wrapped_history_rows(&history_lines, history_content_width);
    let visible_history_rows = chunks[1].height.saturating_sub(2) as usize;
    state.set_history_layout(total_history_rows, visible_history_rows);
    let history_scroll = state.effective_history_scroll();
    let history_scroll = u16::try_from(history_scroll).unwrap_or(u16::MAX);
    let history_title =
        "History (Wheel/Up/Down scroll, PgUp/PgDn page, Ctrl+Home top, Ctrl+End follow)";

    let history = Paragraph::new(history_lines)
        .block(Block::default().borders(Borders::ALL).title(history_title))
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
        Span::raw(" | "),
        Span::styled(
            "session: ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(state.session_id.clone(), Style::default().fg(Color::Green)),
        Span::raw(" | "),
        Span::styled(
            "state: ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            state.session_state.clone(),
            Style::default().fg(Color::Yellow),
        ),
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

fn total_wrapped_history_rows(lines: &[Line<'static>], content_width: u16) -> usize {
    if content_width == 0 {
        return lines.len();
    }

    let width = usize::from(content_width);
    lines
        .iter()
        .map(|line| {
            let row_width = line
                .spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum::<usize>();
            let row_width = row_width.max(1);
            row_width.div_ceil(width)
        })
        .sum()
}

fn append_message_lines(lines: &mut Vec<Line<'static>>, message: &ChatMessage) {
    let prefix = role_prefix(message.role);
    let continuation = " ".repeat(prefix.chars().count());
    let role_style = role_prefix_style(message.role);
    let body_style = role_body_style(message.role);

    if message.role == MessageRole::Assistant {
        append_assistant_markdown_lines(
            lines,
            message,
            prefix,
            continuation,
            role_style,
            body_style,
        );
        return;
    }

    append_plain_message_lines(lines, message, prefix, continuation, role_style, body_style);
}

fn append_plain_message_lines(
    lines: &mut Vec<Line<'static>>,
    message: &ChatMessage,
    prefix: String,
    continuation: String,
    role_style: Style,
    body_style: Style,
) {
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

fn append_assistant_markdown_lines(
    lines: &mut Vec<Line<'static>>,
    message: &ChatMessage,
    prefix: String,
    continuation: String,
    role_style: Style,
    body_style: Style,
) {
    let rendered_lines = render_markdown_lines(&message.text, body_style);
    if rendered_lines.is_empty() {
        lines.push(Line::from(Span::styled(prefix, role_style)));
        return;
    }

    let mut rendered_iter = rendered_lines.into_iter();
    if let Some(first_line) = rendered_iter.next() {
        let mut first_spans = vec![Span::styled(prefix.clone(), role_style)];
        first_spans.extend(first_line.spans);
        lines.push(Line::from(first_spans));
    }

    for line in rendered_iter {
        let mut spans = vec![Span::raw(continuation.clone())];
        spans.extend(line.spans);
        lines.push(Line::from(spans));
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
    fn history_lines_render_assistant_markdown_lists_and_code_fences() {
        let messages = vec![ChatMessage {
            role: MessageRole::Assistant,
            text: "- item one\n- item two\n\n```rust\nlet value = 42;\n```".to_string(),
            timestamp: 1,
        }];

        let lines = build_history_lines(&messages);

        assert_eq!(line_text(&lines[0]), ". - item one");
        assert_eq!(line_text(&lines[1]), "  - item two");
        assert_eq!(line_text(&lines[2]), "  ");
        assert_eq!(line_text(&lines[3]), "      let value = 42;");
    }

    #[test]
    fn model_variant_line_shows_current_selection() {
        let state = AppState {
            model: "openai/gpt-5.3-codex".to_string(),
            variant: "xhigh".to_string(),
            session_id: "session-123".to_string(),
            session_state: "restored".to_string(),
            ..AppState::default()
        };

        let line = build_model_variant_line(&state);

        assert_eq!(
            line_text(&line),
            "model: openai/gpt-5.3-codex | variant: xhigh | session: session-123 | state: restored"
        );
    }

    #[test]
    fn total_wrapped_history_rows_counts_wrapped_content() {
        let lines = vec![Line::from("this is a long wrapped line")];

        let wrapped_rows = total_wrapped_history_rows(&lines, 8);
        let unwrapped_rows = total_wrapped_history_rows(&lines, 80);

        assert!(wrapped_rows > unwrapped_rows);
        assert_eq!(unwrapped_rows, 1);
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }
}
