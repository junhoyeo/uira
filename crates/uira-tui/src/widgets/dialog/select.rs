use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::Theme;

use super::{DialogContent, DialogResult};

pub struct DialogSelectItem<T> {
    pub title: String,
    pub value: T,
    pub description: Option<String>,
    pub category: Option<String>,
    pub footer: Option<String>,
    pub disabled: bool,
}

impl<T> DialogSelectItem<T> {
    pub fn new(title: impl Into<String>, value: T) -> Self {
        Self {
            title: title.into(),
            value,
            description: None,
            category: None,
            footer: None,
            disabled: false,
        }
    }
}

pub struct DialogSelect<T: Clone + 'static> {
    title: String,
    placeholder: String,
    filter: String,
    items: Vec<DialogSelectItem<T>>,
    filtered_indices: Vec<usize>,
    selected: usize,
    footer_hint: String,
    on_select: Option<Box<dyn FnMut(T)>>,
}

impl<T: Clone + 'static> DialogSelect<T> {
    pub fn new(title: impl Into<String>, items: Vec<DialogSelectItem<T>>) -> Self {
        let mut this = Self {
            title: title.into(),
            placeholder: "Search".to_string(),
            filter: String::new(),
            items,
            filtered_indices: Vec::new(),
            selected: 0,
            footer_hint: "Up/Down navigate | Enter select | Esc close".to_string(),
            on_select: None,
        };
        this.rebuild_filter();
        this
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn with_footer_hint(mut self, hint: impl Into<String>) -> Self {
        self.footer_hint = hint.into();
        self
    }

    pub fn on_select(mut self, callback: impl FnMut(T) + 'static) -> Self {
        self.on_select = Some(Box::new(callback));
        self
    }

    pub fn selected_value(&self) -> Option<T> {
        self.filtered_indices
            .get(self.selected)
            .and_then(|idx| self.items.get(*idx))
            .map(|item| item.value.clone())
    }

    fn move_selection(&mut self, delta: isize) {
        if self.filtered_indices.is_empty() {
            self.selected = 0;
            return;
        }
        let max = self.filtered_indices.len() as isize;
        let mut next = self.selected as isize + delta;
        if next < 0 {
            next = max - 1;
        } else if next >= max {
            next = 0;
        }
        self.selected = next as usize;
    }

    fn rebuild_filter(&mut self) {
        let needle = self.filter.to_lowercase();
        let mut scored: Vec<(usize, i64)> = Vec::new();

        for (idx, item) in self.items.iter().enumerate() {
            if item.disabled {
                continue;
            }

            let category = item.category.as_deref().unwrap_or_default();
            if needle.is_empty() {
                scored.push((idx, 0));
                continue;
            }

            let title_score = fuzzy_score(&item.title.to_lowercase(), &needle).map(|s| s + 1000);
            let cat_score = fuzzy_score(&category.to_lowercase(), &needle);
            let desc_score = item
                .description
                .as_ref()
                .and_then(|desc| fuzzy_score(&desc.to_lowercase(), &needle));

            if let Some(score) = [title_score, cat_score, desc_score]
                .into_iter()
                .flatten()
                .max()
            {
                scored.push((idx, score));
            }
        }

        scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        self.filtered_indices = scored.into_iter().map(|(idx, _)| idx).collect();
        if self.selected >= self.filtered_indices.len() {
            self.selected = 0;
        }
    }
}

impl<T: Clone + 'static> DialogContent for DialogSelect<T> {
    fn desired_size(&self, viewport: Rect) -> (u16, u16) {
        let width = 72u16.min(viewport.width.saturating_sub(4));
        let height = 24u16.min(viewport.height.saturating_sub(4));
        (width, height)
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
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(1),
            ])
            .split(inner);

        let filter_text = if self.filter.is_empty() {
            Span::styled(&self.placeholder, Style::default().fg(theme.text_muted))
        } else {
            Span::styled(&self.filter, Style::default().fg(theme.fg))
        };
        let filter_line = Paragraph::new(Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(theme.accent)),
            filter_text,
        ]));
        frame.render_widget(filter_line, rows[0]);

        let divider = Paragraph::new("-".repeat(rows[1].width as usize))
            .style(Style::default().fg(theme.border_subtle));
        frame.render_widget(divider, rows[1]);

        let list_area = rows[2];
        let visible_height = list_area.height as usize;
        let selected_row = self
            .filtered_indices
            .iter()
            .enumerate()
            .take(self.selected + 1)
            .fold(
                (String::new(), 0usize),
                |(mut current_cat, row), (index, item_idx)| {
                    let item = &self.items[*item_idx];
                    let mut total = row;
                    let cat = item.category.clone().unwrap_or_default();
                    if cat != current_cat && !cat.is_empty() {
                        total += 1;
                        current_cat = cat;
                    }
                    if index <= self.selected {
                        total += 1;
                    }
                    (current_cat, total)
                },
            )
            .1
            .saturating_sub(1);

        let scroll = if selected_row >= visible_height && visible_height > 0 {
            selected_row.saturating_sub(visible_height - 1)
        } else {
            0
        };

        let mut items: Vec<ListItem> = Vec::new();
        let mut current_category = String::new();
        let mut visual_row = 0usize;

        for (flat_idx, idx) in self.filtered_indices.iter().enumerate() {
            let item = &self.items[*idx];
            let category = item.category.clone().unwrap_or_default();
            if category != current_category && !category.is_empty() {
                if visual_row >= scroll && items.len() < visible_height {
                    items.push(ListItem::new(Line::from(Span::styled(
                        format!("  {}", category),
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ))));
                }
                current_category = category;
                visual_row += 1;
            }

            if visual_row >= scroll && items.len() < visible_height {
                let selected = flat_idx == self.selected;
                let mut left = item.title.clone();
                if let Some(desc) = &item.description {
                    left.push(' ');
                    left.push_str(desc);
                }

                let footer = item.footer.clone().unwrap_or_default();
                let usable = list_area.width.saturating_sub(4) as usize;
                let left_width = usable.saturating_sub(footer.len() + 1);
                if left.len() > left_width {
                    left = truncate_with_ellipsis(&left, left_width);
                }
                let padding = usable.saturating_sub(left.len() + footer.len());

                let item_style = if selected {
                    Style::default()
                        .bg(theme.accent)
                        .fg(Theme::contrast_text(theme.accent))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.fg)
                };
                let footer_style = if selected {
                    Style::default()
                        .bg(theme.accent)
                        .fg(Theme::contrast_text(theme.accent))
                } else {
                    Style::default().fg(theme.text_muted)
                };

                items.push(ListItem::new(Line::from(vec![
                    Span::styled(format!("  {}", left), item_style),
                    Span::styled(" ".repeat(padding), item_style),
                    Span::styled(footer, footer_style),
                ])));
            }
            visual_row += 1;
        }

        if items.is_empty() {
            items.push(ListItem::new(Line::from(Span::styled(
                "  No results",
                Style::default().fg(theme.text_muted),
            ))));
        }

        frame.render_widget(List::new(items), list_area);

        let hint = Paragraph::new(self.footer_hint.as_str())
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme.text_muted));
        frame.render_widget(hint, rows[3]);
    }

    fn handle_key(&mut self, key: KeyCode) -> DialogResult {
        match key {
            KeyCode::Esc => DialogResult::Close,
            KeyCode::Up => {
                self.move_selection(-1);
                DialogResult::None
            }
            KeyCode::Down => {
                self.move_selection(1);
                DialogResult::None
            }
            KeyCode::Enter => {
                if let Some(value) = self.selected_value() {
                    if let Some(on_select) = self.on_select.as_mut() {
                        on_select(value);
                    }
                }
                DialogResult::Close
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.rebuild_filter();
                DialogResult::None
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.rebuild_filter();
                DialogResult::None
            }
            _ => DialogResult::None,
        }
    }
}

fn fuzzy_score(haystack: &str, needle: &str) -> Option<i64> {
    if needle.is_empty() {
        return Some(0);
    }
    let mut score = 0i64;
    let mut needle_chars = needle.chars();
    let mut current = needle_chars.next()?;
    let mut consecutive = 0i64;

    for ch in haystack.chars() {
        if ch == current {
            consecutive += 1;
            score += 10 + consecutive * 2;
            if let Some(next) = needle_chars.next() {
                current = next;
            } else {
                return Some(score);
            }
        } else {
            consecutive = 0;
            score -= 1;
        }
    }

    None
}

fn truncate_with_ellipsis(input: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let count = input.chars().count();
    if count <= max {
        return input.to_string();
    }
    if max <= 3 {
        return ".".repeat(max);
    }
    input.chars().take(max - 3).collect::<String>() + "..."
}
