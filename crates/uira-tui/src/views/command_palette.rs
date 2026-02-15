//! Command palette overlay for fuzzy-searchable command list

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::Theme;

#[derive(Clone, Debug)]
pub struct PaletteCommand {
    pub id: String,
    pub title: String,
    pub category: String,
    pub keybind: Option<String>,
    pub slash: Option<String>,
}

pub enum PaletteAction {
    Execute(String),
    Close,
    None,
}

pub struct CommandPalette {
    active: bool,
    query: String,
    commands: Vec<PaletteCommand>,
    filtered: Vec<usize>,
    selected: usize,
    theme: Theme,
}

impl CommandPalette {
    pub fn new() -> Self {
        let commands = Self::default_commands();
        let filtered: Vec<usize> = (0..commands.len()).collect();
        Self {
            active: false,
            query: String::new(),
            commands,
            filtered,
            selected: 0,
            theme: Theme::default(),
        }
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn open(&mut self) {
        self.active = true;
        self.query.clear();
        self.selected = 0;
        self.filtered = (0..self.commands.len()).collect();
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    pub fn handle_key(&mut self, key: KeyCode) -> PaletteAction {
        if !self.active {
            return PaletteAction::None;
        }

        match key {
            KeyCode::Esc => {
                self.close();
                PaletteAction::Close
            }
            KeyCode::Enter => {
                if let Some(&idx) = self.filtered.get(self.selected) {
                    let id = self.commands[idx].id.clone();
                    self.close();
                    PaletteAction::Execute(id)
                } else {
                    PaletteAction::None
                }
            }
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                PaletteAction::None
            }
            KeyCode::Down => {
                if !self.filtered.is_empty() && self.selected < self.filtered.len() - 1 {
                    self.selected += 1;
                }
                PaletteAction::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.update_filter();
                PaletteAction::None
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                self.update_filter();
                PaletteAction::None
            }
            _ => PaletteAction::None,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.active {
            return;
        }

        let width = 60u16.min(area.width.saturating_sub(4));
        let height = 20u16.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let modal_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .title(" Command Palette ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner);

        let input_display = if self.query.is_empty() {
            Span::styled("Type to filter...", Style::default().fg(self.theme.borders))
        } else {
            Span::styled(&self.query, Style::default().fg(self.theme.fg))
        };
        let input_line = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(self.theme.accent)),
            input_display,
        ]));
        frame.render_widget(input_line, chunks[0]);

        let sep = Paragraph::new("─".repeat(inner.width as usize))
            .style(Style::default().fg(self.theme.borders));
        frame.render_widget(sep, chunks[1]);

        let list_area = chunks[2];
        let max_items = list_area.height as usize;
        let mut items: Vec<ListItem> = Vec::new();
        let mut current_category = String::new();
        let mut item_count = 0usize;

        let mut selected_visual_row = 0usize;
        {
            let mut cat = String::new();
            for (fi, &cmd_idx) in self.filtered.iter().enumerate() {
                let cmd = &self.commands[cmd_idx];
                if cmd.category != cat {
                    selected_visual_row += 1;
                    cat.clone_from(&cmd.category);
                }
                if fi == self.selected {
                    break;
                }
                selected_visual_row += 1;
            }
        }

        let scroll_offset = if selected_visual_row >= max_items {
            selected_visual_row - max_items + 2
        } else {
            0
        };

        let mut visual_row = 0usize;
        for (fi, &cmd_idx) in self.filtered.iter().enumerate() {
            let cmd = &self.commands[cmd_idx];

            if cmd.category != current_category {
                if visual_row >= scroll_offset && item_count < max_items {
                    items.push(ListItem::new(Line::from(Span::styled(
                        format!("  {}", cmd.category),
                        Style::default()
                            .fg(self.theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ))));
                    item_count += 1;
                }
                visual_row += 1;
                current_category.clone_from(&cmd.category);
            }

            if visual_row >= scroll_offset && item_count < max_items {
                let is_selected = fi == self.selected;

                let hint = match (&cmd.keybind, &cmd.slash) {
                    (Some(kb), _) => kb.clone(),
                    (_, Some(sl)) => sl.clone(),
                    _ => String::new(),
                };

                let available = (inner.width as usize).saturating_sub(8);
                let title_width = available.saturating_sub(hint.len() + 1);
                let title_truncated = if cmd.title.len() > title_width {
                    format!("{}…", &cmd.title[..title_width.saturating_sub(1)])
                } else {
                    cmd.title.clone()
                };
                let padding = available.saturating_sub(title_truncated.len() + hint.len());

                let style = if is_selected {
                    Style::default()
                        .fg(Theme::contrast_text(self.theme.accent))
                        .bg(self.theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.fg)
                };

                let hint_style = if is_selected {
                    Style::default()
                        .fg(Theme::contrast_text(self.theme.accent))
                        .bg(self.theme.accent)
                } else {
                    Style::default().fg(self.theme.borders)
                };

                items.push(ListItem::new(Line::from(vec![
                    Span::styled(format!("    {}", title_truncated), style),
                    Span::styled(" ".repeat(padding), style),
                    Span::styled(hint, hint_style),
                ])));
                item_count += 1;
            }
            visual_row += 1;
        }

        if items.is_empty() {
            items.push(ListItem::new(Line::from(Span::styled(
                "  No matching commands",
                Style::default().fg(self.theme.borders),
            ))));
        }

        let list = List::new(items);
        frame.render_widget(list, list_area);

        let help = Paragraph::new("↑↓: navigate | Enter: execute | Esc: close")
            .style(Style::default().fg(self.theme.borders))
            .alignment(Alignment::Center);
        frame.render_widget(help, chunks[3]);
    }

    fn update_filter(&mut self) {
        let query_lower = self.query.to_lowercase();
        self.filtered = self
            .commands
            .iter()
            .enumerate()
            .filter(|(_, cmd)| {
                if query_lower.is_empty() {
                    return true;
                }
                cmd.title.to_lowercase().contains(&query_lower)
                    || cmd.category.to_lowercase().contains(&query_lower)
                    || cmd
                        .slash
                        .as_ref()
                        .is_some_and(|s| s.to_lowercase().contains(&query_lower))
                    || cmd
                        .keybind
                        .as_ref()
                        .is_some_and(|k| k.to_lowercase().contains(&query_lower))
            })
            .map(|(i, _)| i)
            .collect();

        self.selected = 0;
    }

    fn default_commands() -> Vec<PaletteCommand> {
        vec![
            // Navigation
            PaletteCommand {
                id: "help".into(),
                title: "Help".into(),
                category: "Navigation".into(),
                keybind: None,
                slash: Some("/help".into()),
            },
            PaletteCommand {
                id: "status".into(),
                title: "Status".into(),
                category: "Navigation".into(),
                keybind: None,
                slash: Some("/status".into()),
            },
            PaletteCommand {
                id: "clear".into(),
                title: "Clear Chat".into(),
                category: "Navigation".into(),
                keybind: None,
                slash: Some("/clear".into()),
            },
            PaletteCommand {
                id: "exit".into(),
                title: "Exit".into(),
                category: "Navigation".into(),
                keybind: None,
                slash: Some("/exit".into()),
            },
            // Model
            PaletteCommand {
                id: "models".into(),
                title: "Open Model Selector".into(),
                category: "Model".into(),
                keybind: None,
                slash: Some("/models".into()),
            },
            PaletteCommand {
                id: "model".into(),
                title: "Switch Model".into(),
                category: "Model".into(),
                keybind: None,
                slash: Some("/model".into()),
            },
            // Theme
            PaletteCommand {
                id: "theme_list".into(),
                title: "List Themes".into(),
                category: "Theme".into(),
                keybind: None,
                slash: Some("/theme".into()),
            },
            // Session
            PaletteCommand {
                id: "fork".into(),
                title: "Fork Session".into(),
                category: "Session".into(),
                keybind: None,
                slash: Some("/fork".into()),
            },
            PaletteCommand {
                id: "switch".into(),
                title: "Switch Branch".into(),
                category: "Session".into(),
                keybind: None,
                slash: Some("/switch".into()),
            },
            PaletteCommand {
                id: "branches".into(),
                title: "List Branches".into(),
                category: "Session".into(),
                keybind: None,
                slash: Some("/branches".into()),
            },
            PaletteCommand {
                id: "tree".into(),
                title: "Branch Tree".into(),
                category: "Session".into(),
                keybind: None,
                slash: Some("/tree".into()),
            },
            PaletteCommand {
                id: "share".into(),
                title: "Share Session".into(),
                category: "Session".into(),
                keybind: None,
                slash: Some("/share".into()),
            },
            PaletteCommand {
                id: "review".into(),
                title: "Review Changes".into(),
                category: "Session".into(),
                keybind: None,
                slash: Some("/review".into()),
            },
            // Tools
            PaletteCommand {
                id: "collapse_tools".into(),
                title: "Collapse All Tool Outputs".into(),
                category: "Tools".into(),
                keybind: Some("Ctrl+O".into()),
                slash: None,
            },
            PaletteCommand {
                id: "expand_tools".into(),
                title: "Expand All Tool Outputs".into(),
                category: "Tools".into(),
                keybind: Some("Ctrl+Shift+O".into()),
                slash: None,
            },
            PaletteCommand {
                id: "toggle_sidebar".into(),
                title: "Toggle Todo Sidebar".into(),
                category: "Tools".into(),
                keybind: Some("t".into()),
                slash: None,
            },
            // Image
            PaletteCommand {
                id: "image".into(),
                title: "Attach Image".into(),
                category: "Image".into(),
                keybind: None,
                slash: Some("/image".into()),
            },
            PaletteCommand {
                id: "screenshot".into(),
                title: "Capture Screenshot".into(),
                category: "Image".into(),
                keybind: None,
                slash: Some("/screenshot".into()),
            },
        ]
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}
