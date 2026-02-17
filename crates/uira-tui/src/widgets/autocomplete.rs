use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::fs;
use std::path::{Path, PathBuf};

use crate::frecency::FrecencyStore;
use crate::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutocompleteMode {
    None,
    File,
    Slash,
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub value: String,
    pub description: String,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub command: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Default, Clone)]
pub struct AutocompleteState {
    pub mode: AutocompleteMode,
    pub selected: usize,
    pub suggestions: Vec<Suggestion>,
    pub token_start: usize,
    pub token_end: usize,
}

impl Default for AutocompleteMode {
    fn default() -> Self {
        Self::None
    }
}

impl AutocompleteState {
    pub fn clear(&mut self) {
        self.mode = AutocompleteMode::None;
        self.selected = 0;
        self.suggestions.clear();
        self.token_start = 0;
        self.token_end = 0;
    }

    pub fn is_active(&self) -> bool {
        self.mode != AutocompleteMode::None && !self.suggestions.is_empty()
    }

    pub fn next(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.suggestions.len();
    }

    pub fn prev(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.suggestions.len() - 1
        } else {
            self.selected - 1
        };
    }

    pub fn selected_value(&self) -> Option<String> {
        self.suggestions.get(self.selected).map(|s| s.value.clone())
    }

    pub fn update(
        &mut self,
        input: &str,
        cursor_pos: usize,
        working_directory: &str,
        frecency: &FrecencyStore,
        slash_commands: &[SlashCommand],
    ) {
        self.clear();

        let chars: Vec<char> = input.chars().collect();
        let cursor = cursor_pos.min(chars.len());

        if input.starts_with('/') {
            let token = chars[..cursor].iter().collect::<String>();
            if !token.contains(' ') && !token.contains('\n') {
                self.mode = AutocompleteMode::Slash;
                self.token_start = 0;
                self.token_end = cursor;
                let query = token.trim_start_matches('/').to_lowercase();
                self.suggestions = slash_commands
                    .iter()
                    .filter_map(|cmd| {
                        let command = cmd.command.to_string();
                        let score = fuzzy_score(&query, &command)?;
                        let fq = format!("/{}", command);
                        let frecency_key = format!("command:{}", fq);
                        let combined = score + frecency.score(&frecency_key);
                        Some(Suggestion {
                            value: fq,
                            description: cmd.description.to_string(),
                            score: combined,
                        })
                    })
                    .collect();
                self.suggestions.sort_by(|a, b| {
                    b.score
                        .total_cmp(&a.score)
                        .then_with(|| a.value.cmp(&b.value))
                });
                self.suggestions.truncate(8);
                return;
            }
        }

        let mut start = cursor;
        while start > 0 {
            let c = chars[start - 1];
            if c.is_whitespace() {
                break;
            }
            start -= 1;
        }

        let token: String = chars[start..cursor].iter().collect();
        if !token.starts_with('@') {
            return;
        }

        self.mode = AutocompleteMode::File;
        self.token_start = start;
        self.token_end = cursor;

        let raw_query = token.trim_start_matches('@');
        let (file_query, range_suffix) = match raw_query.split_once('#') {
            Some((left, right)) => (left, Some(right)),
            None => (raw_query, None),
        };

        let candidates = collect_files(working_directory, 1200);
        let query_lower = file_query.to_lowercase();
        let mut ranked: Vec<Suggestion> = candidates
            .into_iter()
            .filter_map(|path| {
                let path_lower = path.to_lowercase();
                let mut score = fuzzy_score(&query_lower, &path_lower)?;
                score += frecency.score(&format!("file:{}", path));
                let mut value = format!("@{}", path);
                if let Some(range) = range_suffix {
                    if !range.is_empty() {
                        value.push('#');
                        value.push_str(range);
                    }
                }
                Some(Suggestion {
                    value,
                    description: "file".to_string(),
                    score,
                })
            })
            .collect();

        ranked.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.value.cmp(&b.value))
        });
        ranked.truncate(8);
        self.suggestions = ranked;
    }

    pub fn render(&self, frame: &mut Frame, input_area: Rect, theme: &Theme) {
        if !self.is_active() {
            return;
        }

        let width = input_area.width.min(64);
        let height = (self.suggestions.len() as u16 + 2).min(10);
        let x = input_area.x;
        let y = input_area.y.saturating_sub(height);
        let area = Rect::new(x, y, width, height);

        let title = match self.mode {
            AutocompleteMode::File => " @ files ",
            AutocompleteMode::Slash => " / commands ",
            AutocompleteMode::None => "",
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent));
        let items: Vec<ListItem> = self
            .suggestions
            .iter()
            .map(|s| {
                let line = Line::from(vec![
                    Span::styled(s.value.clone(), Style::default().fg(theme.fg)),
                    Span::raw(" "),
                    Span::styled(
                        s.description.clone(),
                        Style::default()
                            .fg(theme.text_muted)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .fg(Theme::contrast_text(theme.accent))
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        let mut state = ListState::default().with_selected(Some(self.selected));
        frame.render_stateful_widget(list, area, &mut state);
    }
}

fn collect_files(root: &str, cap: usize) -> Vec<String> {
    let root_path = Path::new(root);
    let mut out = Vec::new();
    walk(root_path, root_path, &mut out, cap);
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>, cap: usize) {
    if out.len() >= cap {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        if out.len() >= cap {
            break;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "target" || name == ".git" {
            continue;
        }

        let Ok(meta) = entry.metadata() else {
            continue;
        };

        if meta.is_dir() {
            walk(root, &path, out, cap);
            continue;
        }

        if meta.is_file() {
            let rel = relativize(root, &path);
            out.push(rel);
        }
    }
}

fn relativize(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn fuzzy_score(query: &str, target: &str) -> Option<f64> {
    if query.is_empty() {
        return Some(1.0);
    }

    let q: Vec<char> = query.chars().collect();
    let t: Vec<char> = target.chars().collect();
    if q.len() > t.len() {
        return None;
    }

    let mut qi = 0usize;
    let mut last_match: Option<usize> = None;
    let mut score = 0.0;

    for (ti, tc) in t.iter().enumerate() {
        if qi >= q.len() {
            break;
        }
        if q[qi].eq_ignore_ascii_case(tc) {
            score += 5.0;
            if let Some(last) = last_match {
                if ti == last + 1 {
                    score += 3.0;
                }
            }
            if ti == 0 || t[ti.saturating_sub(1)] == '/' || t[ti.saturating_sub(1)] == '_' {
                score += 2.0;
            }
            last_match = Some(ti);
            qi += 1;
        }
    }

    if qi == q.len() {
        score += (100.0 / (target.len().max(1) as f64)).min(20.0);
        Some(score)
    } else {
        None
    }
}

#[allow(dead_code)]
fn with_file_range(path: &Path, range: Option<(usize, usize)>) -> String {
    let display = path.to_string_lossy().replace('\\', "/");
    match range {
        Some((start, end)) if end >= start => format!("{}#{}-{}", display, start, end),
        Some((line, _)) => format!("{}#{}", display, line),
        None => display,
    }
}

#[allow(dead_code)]
fn parse_line_range(range: &str) -> Option<(usize, usize)> {
    if range.is_empty() {
        return None;
    }
    if let Some((a, b)) = range.split_once('-') {
        let start = a.parse::<usize>().ok()?;
        let end = b.parse::<usize>().ok()?;
        return Some((start, end));
    }
    let line = range.parse::<usize>().ok()?;
    Some((line, line))
}

#[allow(dead_code)]
fn _normalize(path: PathBuf) -> String {
    path.to_string_lossy().replace('\\', "/")
}
