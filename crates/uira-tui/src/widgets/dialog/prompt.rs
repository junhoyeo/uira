use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::Theme;

use super::{DialogContent, DialogResult};

type Validator = dyn Fn(&str) -> Result<(), String>;

pub struct DialogPrompt {
    title: String,
    placeholder: String,
    value: String,
    cursor: usize,
    error: Option<String>,
    validator: Option<Box<Validator>>,
    on_submit: Option<Box<dyn FnMut(Option<String>)>>,
}

impl DialogPrompt {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            placeholder: "Enter value".to_string(),
            value: String::new(),
            cursor: 0,
            error: None,
            validator: None,
            on_submit: None,
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self.cursor = self.value.chars().count();
        self
    }

    pub fn with_validator(
        mut self,
        validator: impl Fn(&str) -> Result<(), String> + 'static,
    ) -> Self {
        self.validator = Some(Box::new(validator));
        self
    }

    pub fn on_submit(mut self, callback: impl FnMut(Option<String>) + 'static) -> Self {
        self.on_submit = Some(Box::new(callback));
        self
    }

    fn submit(&mut self, value: Option<String>) -> DialogResult {
        if let Some(on_submit) = self.on_submit.as_mut() {
            on_submit(value);
        }
        DialogResult::Close
    }

    fn validate(&self) -> Result<(), String> {
        if let Some(validator) = self.validator.as_ref() {
            validator(&self.value)
        } else {
            Ok(())
        }
    }

    fn byte_index(&self) -> usize {
        self.value
            .char_indices()
            .nth(self.cursor)
            .map(|(idx, _)| idx)
            .unwrap_or(self.value.len())
    }

    fn remove_prev_char(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let target = self.cursor - 1;
        let start = self
            .value
            .char_indices()
            .nth(target)
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        let end = self.byte_index();
        self.value.replace_range(start..end, "");
        self.cursor = target;
    }

    fn remove_char_at_cursor(&mut self) {
        let start = self.byte_index();
        if start >= self.value.len() {
            return;
        }
        let end = self
            .value
            .char_indices()
            .skip(self.cursor)
            .nth(1)
            .map(|(idx, _)| idx)
            .unwrap_or(self.value.len());
        self.value.replace_range(start..end, "");
    }
}

impl DialogContent for DialogPrompt {
    fn desired_size(&self, viewport: Rect) -> (u16, u16) {
        (68u16.min(viewport.width.saturating_sub(4)), 10)
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
                Constraint::Length(2),
                Constraint::Length(1),
            ])
            .split(inner);

        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.borders));
        let input_inner = input_block.inner(rows[0]);
        frame.render_widget(input_block, rows[0]);

        let mut visual = self.value.clone();
        let cursor_byte = self.byte_index();
        if cursor_byte <= visual.len() {
            visual.insert(cursor_byte, '|');
        }
        let input_line = if self.value.is_empty() {
            Line::from(Span::styled(
                self.placeholder.as_str(),
                Style::default().fg(theme.text_muted),
            ))
        } else {
            Line::from(Span::styled(visual, Style::default().fg(theme.fg)))
        };
        frame.render_widget(Paragraph::new(input_line), input_inner);

        let status_text = if let Some(error) = &self.error {
            Span::styled(
                error,
                Style::default()
                    .fg(theme.error)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                "Enter submit | Esc cancel",
                Style::default().fg(theme.text_muted),
            )
        };
        frame.render_widget(Paragraph::new(Line::from(status_text)), rows[1]);

        frame.render_widget(
            Paragraph::new("Left/Right move cursor | Backspace/Delete edit")
                .style(Style::default().fg(theme.text_muted)),
            rows[2],
        );
    }

    fn handle_key(&mut self, key: KeyCode) -> DialogResult {
        match key {
            KeyCode::Esc => self.submit(None),
            KeyCode::Enter => match self.validate() {
                Ok(()) => self.submit(Some(self.value.clone())),
                Err(err) => {
                    self.error = Some(err);
                    DialogResult::None
                }
            },
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                self.error = None;
                DialogResult::None
            }
            KeyCode::Right => {
                if self.cursor < self.value.chars().count() {
                    self.cursor += 1;
                }
                self.error = None;
                DialogResult::None
            }
            KeyCode::Home => {
                self.cursor = 0;
                self.error = None;
                DialogResult::None
            }
            KeyCode::End => {
                self.cursor = self.value.chars().count();
                self.error = None;
                DialogResult::None
            }
            KeyCode::Backspace => {
                self.remove_prev_char();
                self.error = None;
                DialogResult::None
            }
            KeyCode::Delete => {
                self.remove_char_at_cursor();
                self.error = None;
                DialogResult::None
            }
            KeyCode::Char(c) => {
                let idx = self.byte_index();
                self.value.insert(idx, c);
                self.cursor += 1;
                self.error = None;
                DialogResult::None
            }
            _ => DialogResult::None,
        }
    }
}
