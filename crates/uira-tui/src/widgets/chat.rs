//! Chat display widget

#![allow(dead_code)] // TUI components - will be used when TUI is complete

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget, Wrap},
};

use crate::Theme;

/// Message for display
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Chat widget for displaying conversation
pub struct ChatWidget<'a> {
    messages: &'a [ChatMessage],
    block: Option<Block<'a>>,
    theme: Theme,
}

impl<'a> ChatWidget<'a> {
    pub fn new(messages: &'a [ChatMessage]) -> Self {
        Self {
            messages,
            block: None,
            theme: Theme::default(),
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    fn render_message(msg: &ChatMessage, theme: &Theme) -> Vec<Line<'static>> {
        let role_style = match msg.role.as_str() {
            "user" => Style::default().fg(theme.accent),
            "assistant" => Style::default().fg(theme.fg),
            "system" => Style::default().fg(theme.warning),
            "error" => Style::default().fg(theme.error),
            _ => Style::default().fg(theme.fg),
        };

        let mut lines = vec![Line::from(vec![Span::styled(
            format!("{}:", msg.role),
            role_style,
        )])];

        for line in msg.content.lines() {
            lines.push(Line::from(line.to_string()));
        }

        lines.push(Line::from(""));
        lines
    }
}

impl Widget for ChatWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner_area = match &self.block {
            Some(block) => {
                let inner = block.inner(area);
                block.clone().render(area, buf);
                inner
            }
            None => area,
        };

        let lines: Vec<Line> = self
            .messages
            .iter()
            .flat_map(|msg| Self::render_message(msg, &self.theme))
            .collect();

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });

        paragraph.render(inner_area, buf);
    }
}
