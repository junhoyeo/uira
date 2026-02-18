use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::Theme;

use super::{DialogContent, DialogResult};

pub struct DialogHelp {
    title: String,
    items: Vec<(String, String)>,
}

impl DialogHelp {
    pub fn new(items: Vec<(String, String)>) -> Self {
        Self {
            title: "Help".to_string(),
            items,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
}

impl Default for DialogHelp {
    fn default() -> Self {
        Self::new(vec![
            ("Ctrl+K".to_string(), "Open command palette".to_string()),
            ("Esc".to_string(), "Close current dialog".to_string()),
            ("Enter".to_string(), "Confirm active action".to_string()),
        ])
    }
}

impl DialogContent for DialogHelp {
    fn desired_size(&self, viewport: Rect) -> (u16, u16) {
        (70u16.min(viewport.width.saturating_sub(4)), 16)
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
            .constraints([Constraint::Min(4), Constraint::Length(1)])
            .split(inner);

        let entries: Vec<ListItem> = self
            .items
            .iter()
            .map(|(key, desc)| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{: <14}", key),
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(desc, Style::default().fg(theme.fg)),
                ]))
            })
            .collect();
        frame.render_widget(List::new(entries), rows[0]);

        frame.render_widget(
            Paragraph::new("Press Enter or Esc to close")
                .style(Style::default().fg(theme.text_muted)),
            rows[1],
        );
    }

    fn handle_key(&mut self, key: KeyCode) -> DialogResult {
        match key {
            KeyCode::Enter | KeyCode::Esc => DialogResult::Close,
            _ => DialogResult::None,
        }
    }
}
