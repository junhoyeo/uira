use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::{
    widgets::dialog::{DialogContent, DialogResult},
    Theme,
};

#[derive(Clone, Debug)]
pub struct McpServerOption {
    pub id: String,
    pub title: String,
    pub enabled: bool,
}

pub struct DialogMcp {
    options: Vec<McpServerOption>,
    selected: usize,
    on_submit: Option<Box<dyn FnMut(Vec<String>)>>,
}

impl DialogMcp {
    pub fn new(options: Vec<McpServerOption>) -> Self {
        Self {
            options,
            selected: 0,
            on_submit: None,
        }
    }

    pub fn on_submit(mut self, callback: impl FnMut(Vec<String>) + 'static) -> Self {
        self.on_submit = Some(Box::new(callback));
        self
    }

    fn submit(&mut self) -> DialogResult {
        if let Some(on_submit) = self.on_submit.as_mut() {
            let enabled = self
                .options
                .iter()
                .filter(|item| item.enabled)
                .map(|item| item.id.clone())
                .collect();
            on_submit(enabled);
        }
        DialogResult::Close
    }
}

impl DialogContent for DialogMcp {
    fn desired_size(&self, viewport: Rect) -> (u16, u16) {
        (72u16.min(viewport.width.saturating_sub(4)), 20)
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(" MCP ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_active));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(inner);

        let entries: Vec<ListItem> = self
            .options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                let selected = index == self.selected;
                let marker = if option.enabled { "[x]" } else { "[ ]" };
                let style = if selected {
                    Style::default()
                        .bg(theme.accent)
                        .fg(Theme::contrast_text(theme.accent))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.fg)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", marker), style),
                    Span::styled(option.title.as_str(), style),
                ]))
            })
            .collect();
        frame.render_widget(List::new(entries), rows[0]);

        frame.render_widget(
            Paragraph::new("Up/Down navigate | Space toggle | Enter apply | Esc close")
                .style(Style::default().fg(theme.text_muted)),
            rows[1],
        );
    }

    fn handle_key(&mut self, key: KeyCode) -> DialogResult {
        match key {
            KeyCode::Esc => DialogResult::Close,
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                DialogResult::None
            }
            KeyCode::Down => {
                if self.selected + 1 < self.options.len() {
                    self.selected += 1;
                }
                DialogResult::None
            }
            KeyCode::Char(' ') => {
                if let Some(current) = self.options.get_mut(self.selected) {
                    current.enabled = !current.enabled;
                }
                DialogResult::None
            }
            KeyCode::Enter => self.submit(),
            _ => DialogResult::None,
        }
    }
}
