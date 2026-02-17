use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::Theme;

use super::{DialogContent, DialogResult};

pub struct DialogConfirm {
    title: String,
    message: String,
    confirm_selected: bool,
    on_submit: Option<Box<dyn FnMut(bool)>>,
}

impl DialogConfirm {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            confirm_selected: false,
            on_submit: None,
        }
    }

    pub fn on_submit(mut self, callback: impl FnMut(bool) + 'static) -> Self {
        self.on_submit = Some(Box::new(callback));
        self
    }

    fn submit(&mut self, value: bool) -> DialogResult {
        if let Some(on_submit) = self.on_submit.as_mut() {
            on_submit(value);
        }
        DialogResult::Close
    }
}

impl DialogContent for DialogConfirm {
    fn desired_size(&self, viewport: Rect) -> (u16, u16) {
        (60u16.min(viewport.width.saturating_sub(4)), 9)
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
            .constraints([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

        frame.render_widget(
            Paragraph::new(self.message.as_str()).style(Style::default().fg(theme.text_muted)),
            rows[0],
        );

        let confirm_style = if self.confirm_selected {
            Style::default()
                .fg(Theme::contrast_text(theme.accent))
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_muted)
        };
        let cancel_style = if !self.confirm_selected {
            Style::default()
                .fg(Theme::contrast_text(theme.accent))
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_muted)
        };

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("[ Cancel ]  ", cancel_style),
                Span::styled("[ Confirm ]", confirm_style),
            ])),
            rows[1],
        );

        frame.render_widget(
            Paragraph::new("Left/Right toggle | Enter submit | Esc cancel")
                .style(Style::default().fg(theme.text_muted)),
            rows[2],
        );
    }

    fn handle_key(&mut self, key: KeyCode) -> DialogResult {
        match key {
            KeyCode::Left | KeyCode::Right => {
                self.confirm_selected = !self.confirm_selected;
                DialogResult::None
            }
            KeyCode::Enter => self.submit(self.confirm_selected),
            KeyCode::Esc => self.submit(false),
            _ => DialogResult::None,
        }
    }

    fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> DialogResult {
        if !matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            return DialogResult::None;
        }

        let x = event.column;
        let y = event.row;
        let action_y = area.y + area.height.saturating_sub(4);
        if y != action_y {
            return DialogResult::None;
        }

        let cancel_start = area.x + 2;
        let cancel_end = cancel_start + 10;
        let confirm_start = cancel_end + 2;
        let confirm_end = confirm_start + 11;

        if x >= cancel_start && x < cancel_end {
            return self.submit(false);
        }
        if x >= confirm_start && x < confirm_end {
            return self.submit(true);
        }

        DialogResult::None
    }
}
