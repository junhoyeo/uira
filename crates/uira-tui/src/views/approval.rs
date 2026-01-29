//! Approval dialog view

#![allow(dead_code)] // TUI components - will be used when TUI is complete

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

/// Approval dialog for tool execution
pub struct ApprovalView<'a> {
    tool_name: &'a str,
    description: &'a str,
    selected: usize, // 0 = Approve, 1 = Deny, 2 = Edit
}

impl<'a> ApprovalView<'a> {
    pub fn new(tool_name: &'a str, description: &'a str) -> Self {
        Self {
            tool_name,
            description,
            selected: 0,
        }
    }

    pub fn select_next(&mut self) {
        self.selected = (self.selected + 1) % 3;
    }

    pub fn select_prev(&mut self) {
        self.selected = if self.selected == 0 {
            2
        } else {
            self.selected - 1
        };
    }

    pub fn selected(&self) -> usize {
        self.selected
    }
}

impl Widget for ApprovalView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear the area first
        Clear.render(area, buf);

        let block = Block::default()
            .title(format!(" Approve: {} ", self.tool_name))
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Yellow));

        let inner = block.inner(area);
        block.render(area, buf);

        let lines = vec![Line::from(self.description), Line::from(""), Line::from("")];

        let content = Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left);

        content.render(inner, buf);

        // Render buttons at the bottom
        let button_y = inner.y + inner.height.saturating_sub(2);
        let buttons = ["[A]pprove", "[D]eny", "[E]dit"];
        let mut x = inner.x + 2;

        for (i, button) in buttons.iter().enumerate() {
            let style = if i == self.selected {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };

            buf.set_string(x, button_y, *button, style);
            x += button.len() as u16 + 2;
        }
    }
}
