//! Model selector overlay for choosing AI models

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use uira_core::{DEFAULT_ANTHROPIC_MODEL, DEFAULT_OPENAI_MODEL};

use crate::Theme;

pub struct ModelGroup {
    pub provider: &'static str,
    pub models: &'static [&'static str],
}

pub const MODEL_GROUPS: &[ModelGroup] = &[
    ModelGroup {
        provider: "opencode",
        models: &[
            "kimi-k2.5-free",
            "glm-4.7",
            "qwen3-coder",
            "claude-opus-4-1",
            "big-pickle",
            "gpt-5-nano",
        ],
    },
    ModelGroup {
        provider: "anthropic",
        models: &[DEFAULT_ANTHROPIC_MODEL, "claude-opus-4-20250514"],
    },
    ModelGroup {
        provider: "openai",
        models: &[DEFAULT_OPENAI_MODEL, "gpt-4o-mini", "o1", "o1-mini"],
    },
    ModelGroup {
        provider: "google",
        models: &["gemini-2.0-flash", "gemini-1.5-pro"],
    },
    ModelGroup {
        provider: "ollama",
        models: &["llama3.1", "qwen2.5-coder", "deepseek-coder"],
    },
];

pub struct ModelSelector {
    active: bool,
    group_index: usize,
    model_index: usize,
    current_model: Option<String>,
    theme: Theme,
}

impl ModelSelector {
    pub fn new() -> Self {
        Self {
            active: false,
            group_index: 0,
            model_index: 0,
            current_model: None,
            theme: Theme::default(),
        }
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn open(&mut self, current_model: Option<String>) {
        self.active = true;
        self.current_model = current_model.clone();

        if let Some(ref model) = current_model {
            for (gi, group) in MODEL_GROUPS.iter().enumerate() {
                for (mi, m) in group.models.iter().enumerate() {
                    let full_name = format!("{}/{}", group.provider, m);
                    if *m == model || full_name == *model {
                        self.group_index = gi;
                        self.model_index = mi;
                        return;
                    }
                }
            }
        }

        self.group_index = 0;
        self.model_index = 0;
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn selected_model(&self) -> Option<(&'static str, &'static str)> {
        MODEL_GROUPS.get(self.group_index).and_then(|group| {
            group
                .models
                .get(self.model_index)
                .map(|model| (group.provider, *model))
        })
    }

    pub fn handle_key(&mut self, key: KeyCode) -> Option<String> {
        if !self.active {
            return None;
        }

        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.close();
                None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.model_index > 0 {
                    self.model_index -= 1;
                } else if self.group_index > 0 {
                    self.group_index -= 1;
                    self.model_index = MODEL_GROUPS[self.group_index].models.len() - 1;
                }
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let current_group = &MODEL_GROUPS[self.group_index];
                if self.model_index < current_group.models.len() - 1 {
                    self.model_index += 1;
                } else if self.group_index < MODEL_GROUPS.len() - 1 {
                    self.group_index += 1;
                    self.model_index = 0;
                }
                None
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if self.group_index > 0 {
                    self.group_index -= 1;
                    self.model_index = 0;
                }
                None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.group_index < MODEL_GROUPS.len() - 1 {
                    self.group_index += 1;
                    self.model_index = 0;
                }
                None
            }
            KeyCode::Enter => {
                if let Some((provider, model)) = self.selected_model() {
                    self.close();
                    Some(format!("{}/{}", provider, model))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.active {
            return;
        }

        let width = 60.min(area.width.saturating_sub(4));
        let height = 20.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let modal_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .title(" Select Model ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.borders));

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(inner);

        let mut items = Vec::new();
        for (gi, group) in MODEL_GROUPS.iter().enumerate() {
            items.push(ListItem::new(Line::from(Span::styled(
                format!("  {}", group.provider.to_uppercase()),
                Style::default()
                    .fg(self.theme.warning)
                    .add_modifier(Modifier::BOLD),
            ))));

            for (mi, model) in group.models.iter().enumerate() {
                let is_selected = gi == self.group_index && mi == self.model_index;
                let full_name = format!("{}/{}", group.provider, model);
                let is_current = self
                    .current_model
                    .as_deref()
                    .map(|cm| cm == *model || cm == full_name)
                    .unwrap_or(false);

                let prefix = if is_current { "→ " } else { "  " };
                let style = if is_selected {
                    Style::default()
                        .fg(Theme::contrast_text(self.theme.accent))
                        .bg(self.theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else if is_current {
                    Style::default()
                        .fg(self.theme.success)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.fg)
                };

                items.push(ListItem::new(Line::from(Span::styled(
                    format!("    {}{}", prefix, model),
                    style,
                ))));
            }
        }

        let list = List::new(items);
        frame.render_widget(list, chunks[0]);

        let help = Paragraph::new("↑↓/jk: navigate | ←→/hl: group | Enter: select | Esc: cancel")
            .style(Style::default().fg(self.theme.borders))
            .alignment(Alignment::Center);
        frame.render_widget(help, chunks[1]);
    }
}

impl Default for ModelSelector {
    fn default() -> Self {
        Self::new()
    }
}
