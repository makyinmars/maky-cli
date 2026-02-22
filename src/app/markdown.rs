use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

#[derive(Debug, Clone, Copy)]
struct ListState {
    next_ordered_number: Option<u64>,
}

#[derive(Debug, Clone)]
struct ListItemState {
    continuation_prefix: String,
}

#[derive(Debug)]
struct MarkdownRenderer {
    lines: Vec<Line<'static>>,
    current_spans: Vec<Span<'static>>,
    base_style: Style,
    emphasis_depth: usize,
    strong_depth: usize,
    strikethrough_depth: usize,
    heading_depth: usize,
    in_code_block: bool,
    code_prefix_pending: bool,
    list_stack: Vec<ListState>,
    list_item_stack: Vec<ListItemState>,
}

impl MarkdownRenderer {
    fn new(base_style: Style) -> Self {
        Self {
            lines: Vec::new(),
            current_spans: Vec::new(),
            base_style,
            emphasis_depth: 0,
            strong_depth: 0,
            strikethrough_depth: 0,
            heading_depth: 0,
            in_code_block: false,
            code_prefix_pending: false,
            list_stack: Vec::new(),
            list_item_stack: Vec::new(),
        }
    }

    fn push_event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.push_text(text.as_ref()),
            Event::Code(text) => self.push_inline_code(text.as_ref()),
            Event::SoftBreak | Event::HardBreak => self.push_line_break(),
            Event::Rule => self.push_rule(),
            Event::TaskListMarker(is_checked) => {
                let marker = if is_checked { "[x] " } else { "[ ] " };
                self.push_styled_text(marker, Self::list_marker_style());
            }
            _ => {}
        }
    }

    fn start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { .. } => {
                self.flush_line_if_non_empty();
                self.heading_depth += 1;
            }
            Tag::CodeBlock(_) => {
                self.flush_line_if_non_empty();
                self.in_code_block = true;
                self.code_prefix_pending = true;
            }
            Tag::List(start_number) => {
                self.flush_line_if_non_empty();
                self.list_stack.push(ListState {
                    next_ordered_number: start_number,
                });
            }
            Tag::Item => self.start_list_item(),
            Tag::Emphasis => self.emphasis_depth += 1,
            Tag::Strong => self.strong_depth += 1,
            Tag::Strikethrough => self.strikethrough_depth += 1,
            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush_line_if_non_empty();
                self.push_block_gap_if_needed();
            }
            TagEnd::Heading(_) => {
                self.heading_depth = self.heading_depth.saturating_sub(1);
                self.flush_line_if_non_empty();
                self.push_block_gap_if_needed();
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                self.code_prefix_pending = false;
                self.flush_line_if_non_empty();
                self.push_block_gap_if_needed();
            }
            TagEnd::List(_) => {
                self.flush_line_if_non_empty();
                self.list_stack.pop();
                self.push_block_gap_if_needed();
            }
            TagEnd::Item => {
                self.flush_line_if_non_empty();
                self.list_item_stack.pop();
            }
            TagEnd::Emphasis => self.emphasis_depth = self.emphasis_depth.saturating_sub(1),
            TagEnd::Strong => self.strong_depth = self.strong_depth.saturating_sub(1),
            TagEnd::Strikethrough => {
                self.strikethrough_depth = self.strikethrough_depth.saturating_sub(1)
            }
            _ => {}
        }
    }

    fn start_list_item(&mut self) {
        if !self.current_spans.is_empty() {
            self.flush_line();
        }

        let depth = self.list_stack.len().saturating_sub(1);
        let indent = "  ".repeat(depth);
        let marker = self.next_list_marker();
        let prefix = format!("{indent}{marker}");
        let continuation_prefix = " ".repeat(prefix.chars().count());

        self.current_spans
            .push(Span::styled(prefix, Self::list_marker_style()));
        self.list_item_stack.push(ListItemState {
            continuation_prefix,
        });
    }

    fn next_list_marker(&mut self) -> String {
        let Some(list_state) = self.list_stack.last_mut() else {
            return "- ".to_string();
        };

        match list_state.next_ordered_number {
            Some(number) => {
                list_state.next_ordered_number = Some(number.saturating_add(1));
                format!("{number}. ")
            }
            None => "- ".to_string(),
        }
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        if self.in_code_block {
            self.push_code_block_text(text);
        } else {
            self.push_styled_text(text, self.current_text_style());
        }
    }

    fn push_inline_code(&mut self, text: &str) {
        self.push_styled_text(text, self.inline_code_style());
    }

    fn push_styled_text(&mut self, text: &str, style: Style) {
        let mut remainder = text;

        loop {
            match remainder.find('\n') {
                Some(index) => {
                    let (segment, rest) = remainder.split_at(index);
                    if !segment.is_empty() {
                        self.current_spans
                            .push(Span::styled(segment.to_string(), style));
                    }
                    self.push_line_break();
                    remainder = &rest[1..];
                }
                None => {
                    if !remainder.is_empty() {
                        self.current_spans
                            .push(Span::styled(remainder.to_string(), style));
                    }
                    break;
                }
            }
        }
    }

    fn push_code_block_text(&mut self, text: &str) {
        let mut remainder = text;
        let style = self.code_block_style();

        loop {
            if self.code_prefix_pending {
                self.current_spans.push(Span::styled("    ", style));
                self.code_prefix_pending = false;
            }

            match remainder.find('\n') {
                Some(index) => {
                    let (segment, rest) = remainder.split_at(index);
                    if !segment.is_empty() {
                        self.current_spans
                            .push(Span::styled(segment.to_string(), style));
                    }
                    self.flush_line();
                    self.code_prefix_pending = true;
                    remainder = &rest[1..];
                }
                None => {
                    if !remainder.is_empty() {
                        self.current_spans
                            .push(Span::styled(remainder.to_string(), style));
                    }
                    break;
                }
            }
        }
    }

    fn push_line_break(&mut self) {
        self.flush_line();
        if self.in_code_block {
            self.code_prefix_pending = true;
            return;
        }
        self.write_list_continuation_prefix();
    }

    fn write_list_continuation_prefix(&mut self) {
        let Some(item_state) = self.list_item_stack.last() else {
            return;
        };
        if item_state.continuation_prefix.is_empty() {
            return;
        }

        self.current_spans
            .push(Span::raw(item_state.continuation_prefix.clone()));
    }

    fn push_rule(&mut self) {
        self.flush_line_if_non_empty();
        self.current_spans
            .push(Span::styled("────────", Self::rule_style()));
        self.flush_line_if_non_empty();
        self.push_block_gap_if_needed();
    }

    fn push_block_gap_if_needed(&mut self) {
        if !self.list_item_stack.is_empty() {
            return;
        }
        if self.lines.last().is_some_and(|line| !line.spans.is_empty()) {
            self.lines.push(Line::default());
        }
    }

    fn flush_line_if_non_empty(&mut self) {
        if self.current_spans.is_empty() {
            return;
        }
        self.flush_line();
    }

    fn flush_line(&mut self) {
        self.lines
            .push(Line::from(std::mem::take(&mut self.current_spans)));
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush_line_if_non_empty();
        while self.lines.last().is_some_and(|line| line.spans.is_empty()) {
            self.lines.pop();
        }
        self.lines
    }

    fn current_text_style(&self) -> Style {
        let mut style = self.base_style;
        if self.heading_depth > 0 {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.strong_depth > 0 {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.emphasis_depth > 0 {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if self.strikethrough_depth > 0 {
            style = style.add_modifier(Modifier::CROSSED_OUT);
        }
        style
    }

    fn inline_code_style(&self) -> Style {
        self.base_style
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    }

    fn code_block_style(&self) -> Style {
        self.base_style.fg(Color::Yellow)
    }

    fn list_marker_style() -> Style {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    }

    fn rule_style() -> Style {
        Style::default().fg(Color::DarkGray)
    }
}

pub fn render_markdown_lines(markdown: &str, base_style: Style) -> Vec<Line<'static>> {
    let mut renderer = MarkdownRenderer::new(base_style);
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;

    for event in Parser::new_ext(markdown, options) {
        renderer.push_event(event);
    }

    renderer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emphasis_and_strong_markers_apply_style_modifiers() {
        let lines = render_markdown_lines(
            "plain *italic* **bold** `code`",
            Style::default().fg(Color::Green),
        );

        assert_eq!(line_text(&lines[0]), "plain italic bold code");
        assert!(span_has_modifier(&lines[0], "italic", Modifier::ITALIC));
        assert!(span_has_modifier(&lines[0], "bold", Modifier::BOLD));
        assert!(span_has_fg_color(&lines[0], "code", Color::Yellow));
    }

    #[test]
    fn bullet_and_ordered_lists_include_markers() {
        let lines =
            render_markdown_lines("- alpha\n- beta\n\n3. gamma\n4. delta", Style::default());

        assert_eq!(line_text(&lines[0]), "- alpha");
        assert_eq!(line_text(&lines[1]), "- beta");
        assert_eq!(line_text(&lines[2]), "");
        assert_eq!(line_text(&lines[3]), "3. gamma");
        assert_eq!(line_text(&lines[4]), "4. delta");
    }

    #[test]
    fn fenced_code_blocks_render_indented_lines() {
        let lines = render_markdown_lines(
            "```rust\nlet value = 42;\nprintln!(\"{value}\");\n```",
            Style::default().fg(Color::Green),
        );

        assert_eq!(line_text(&lines[0]), "    let value = 42;");
        assert_eq!(line_text(&lines[1]), "    println!(\"{value}\");");
        assert!(span_has_fg_color(
            &lines[0],
            "let value = 42;",
            Color::Yellow
        ));
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    fn span_has_modifier(line: &Line<'_>, text: &str, modifier: Modifier) -> bool {
        line.spans.iter().any(|span| {
            span.content.as_ref().contains(text) && span.style.add_modifier.contains(modifier)
        })
    }

    fn span_has_fg_color(line: &Line<'_>, text: &str, color: Color) -> bool {
        line.spans.iter().any(|span| {
            span.content.as_ref().contains(text) && span.style.fg.is_some_and(|fg| fg == color)
        })
    }
}
