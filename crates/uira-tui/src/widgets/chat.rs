//! Chat display widget

#![allow(dead_code)] // TUI components - will be used when TUI is complete

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget, Wrap},
};

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
}

impl<'a> ChatWidget<'a> {
    pub fn new(messages: &'a [ChatMessage]) -> Self {
        Self {
            messages,
            block: None,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    fn render_message(msg: &ChatMessage) -> Vec<Line<'static>> {
        let role_style = match msg.role.as_str() {
            "user" => Style::default().fg(Color::Green),
            "assistant" => Style::default().fg(Color::Blue),
            "system" => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::White),
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
            .flat_map(Self::render_message)
            .collect();

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });

        paragraph.render(inner_area, buf);
    }
}
