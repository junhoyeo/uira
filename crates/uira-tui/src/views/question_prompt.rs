use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::Theme;

#[derive(Clone, Debug)]
pub struct QuestionOption {
    pub label: String,
    pub value: String,
    pub selected: bool,
}

impl QuestionOption {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            selected: false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum QuestionPromptAction {
    None,
    Submit(Vec<String>),
    Cancel,
}

pub struct QuestionPrompt {
    question: String,
    options: Vec<QuestionOption>,
    selected_index: usize,
    multi_select: bool,
    custom_input: String,
    custom_cursor: usize,
    custom_active: bool,
}

impl QuestionPrompt {
    pub fn new(
        question: impl Into<String>,
        options: Vec<QuestionOption>,
        multi_select: bool,
    ) -> Self {
        Self {
            question: question.into(),
            options,
            selected_index: 0,
            multi_select,
            custom_input: String::new(),
            custom_cursor: 0,
            custom_active: false,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(" Question ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_active));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(5),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(inner);

        frame.render_widget(
            Paragraph::new(self.question.as_str()).wrap(Wrap { trim: true }),
            sections[0],
        );

        let mut items = Vec::with_capacity(self.options.len());
        for (idx, option) in self.options.iter().enumerate() {
            let marker = if option.selected { "[x]" } else { "[ ]" };
            let selected = idx == self.selected_index && !self.custom_active;
            let style = if selected {
                Style::default()
                    .bg(theme.accent)
                    .fg(Theme::contrast_text(theme.accent))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            };
            items.push(ListItem::new(Line::from(Span::styled(
                format!(" {} {}", marker, option.label),
                style,
            ))));
        }
        frame.render_widget(List::new(items), sections[1]);

        let mut custom_line = self.custom_input.clone();
        if self.custom_cursor <= custom_line.len() {
            custom_line.insert(self.custom_cursor, '|');
        }
        let custom_text = if self.custom_input.is_empty() {
            "custom answer...".to_string()
        } else {
            custom_line
        };
        let custom_style = if self.custom_active {
            Style::default().fg(theme.accent)
        } else {
            Style::default().fg(theme.text_muted)
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Custom: ", Style::default().fg(theme.warning)),
                Span::styled(custom_text, custom_style),
            ]))
            .block(Block::default().borders(Borders::ALL).border_style(
                if self.custom_active {
                    Style::default().fg(theme.accent)
                } else {
                    Style::default().fg(theme.borders)
                },
            )),
            sections[2],
        );

        let hint = if self.multi_select {
            "Tab switch input | Space toggle | Enter submit | Esc cancel"
        } else {
            "Tab switch input | Enter submit | Esc cancel"
        };
        frame.render_widget(
            Paragraph::new(hint).style(Style::default().fg(theme.text_muted)),
            sections[3],
        );
    }

    pub fn handle_key(&mut self, key: KeyCode) -> QuestionPromptAction {
        if self.custom_active {
            return self.handle_custom_key(key);
        }

        match key {
            KeyCode::Esc => QuestionPromptAction::Cancel,
            KeyCode::Tab => {
                self.custom_active = true;
                QuestionPromptAction::None
            }
            KeyCode::Up => {
                self.selected_index = self.selected_index.saturating_sub(1);
                QuestionPromptAction::None
            }
            KeyCode::Down => {
                self.selected_index =
                    (self.selected_index + 1).min(self.options.len().saturating_sub(1));
                QuestionPromptAction::None
            }
            KeyCode::Char(' ') if self.multi_select => {
                if let Some(option) = self.options.get_mut(self.selected_index) {
                    option.selected = !option.selected;
                }
                QuestionPromptAction::None
            }
            KeyCode::Enter => {
                if !self.multi_select {
                    for option in &mut self.options {
                        option.selected = false;
                    }
                    if let Some(option) = self.options.get_mut(self.selected_index) {
                        option.selected = true;
                    }
                }
                QuestionPromptAction::Submit(self.selected_values())
            }
            _ => QuestionPromptAction::None,
        }
    }

    fn handle_custom_key(&mut self, key: KeyCode) -> QuestionPromptAction {
        match key {
            KeyCode::Esc => QuestionPromptAction::Cancel,
            KeyCode::Tab => {
                self.custom_active = false;
                QuestionPromptAction::None
            }
            KeyCode::Enter => QuestionPromptAction::Submit(self.selected_values()),
            KeyCode::Left => {
                self.custom_cursor = self.custom_cursor.saturating_sub(1);
                QuestionPromptAction::None
            }
            KeyCode::Right => {
                self.custom_cursor = (self.custom_cursor + 1).min(self.custom_input.len());
                QuestionPromptAction::None
            }
            KeyCode::Backspace => {
                if self.custom_cursor > 0 {
                    self.custom_cursor -= 1;
                    self.custom_input.remove(self.custom_cursor);
                }
                QuestionPromptAction::None
            }
            KeyCode::Char(c) => {
                self.custom_input.insert(self.custom_cursor, c);
                self.custom_cursor += 1;
                QuestionPromptAction::None
            }
            _ => QuestionPromptAction::None,
        }
    }

    pub fn selected_values(&self) -> Vec<String> {
        let mut values: Vec<String> = self
            .options
            .iter()
            .filter(|option| option.selected)
            .map(|option| option.value.clone())
            .collect();
        let custom = self.custom_input.trim();
        if !custom.is_empty() {
            values.push(custom.to_string());
        }
        values
    }
}
