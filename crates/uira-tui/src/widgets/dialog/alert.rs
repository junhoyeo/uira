use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::Theme;

use super::{DialogContent, DialogResult};

pub struct DialogAlert {
    title: String,
    message: String,
    on_ack: Option<Box<dyn FnMut()>>,
}

impl DialogAlert {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            on_ack: None,
        }
    }

    pub fn on_ack(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_ack = Some(Box::new(callback));
        self
    }

    fn acknowledge(&mut self) -> DialogResult {
        if let Some(on_ack) = self.on_ack.as_mut() {
            on_ack();
        }
        DialogResult::Close
    }
}

impl DialogContent for DialogAlert {
    fn desired_size(&self, viewport: Rect) -> (u16, u16) {
        (60u16.min(viewport.width.saturating_sub(4)), 8)
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_active));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(1)])
            .split(inner);

        frame.render_widget(
            Paragraph::new(self.message.as_str()).style(Style::default().fg(theme.text_muted)),
            rows[0],
        );

        frame.render_widget(
            Paragraph::new("Press Enter or Esc to close").style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            rows[1],
        );
    }

    fn handle_key(&mut self, key: KeyCode) -> DialogResult {
        match key {
            KeyCode::Enter | KeyCode::Esc => self.acknowledge(),
            _ => DialogResult::None,
        }
    }
}
