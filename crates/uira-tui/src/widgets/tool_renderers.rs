//! Per-tool output renderers for the TUI chat view.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::Theme;

const BASH_MAX_LINES: usize = 15;
const BASH_PREVIEW_LINES: usize = 10;
const READ_PREVIEW_LINES: usize = 10;
const LIST_MAX_ITEMS: usize = 20;
const GENERIC_PREVIEW_LINES: usize = 10;

pub fn render_tool_output(
    tool_name: &str,
    content: &str,
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let normalized = tool_name.to_lowercase();
    match normalized.as_str() {
        "bash" => render_bash(content, width, theme),
        "edit" | "write" => render_edit(content, width, theme),
        "read" => render_read(content, width, theme),
        "glob" | "grep" => render_glob_grep(content, width, theme),
        _ => render_generic(content, width, theme),
    }
}

fn render_bash(content: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let all_lines: Vec<&str> = content.lines().collect();
    if all_lines.is_empty() {
        return vec![Line::from(Span::styled(
            "(no output)".to_string(),
            Style::default().fg(theme.text_muted),
        ))];
    }

    let mut result = Vec::new();

    let cmd_display = format!("  \u{276f} {}", all_lines[0]);
    result.extend(wrap_styled(
        &cmd_display,
        width,
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    ));

    let output_lines = &all_lines[1..];
    let output_style = Style::default().fg(theme.text_muted);

    if output_lines.len() > BASH_MAX_LINES {
        for line in &output_lines[..BASH_PREVIEW_LINES] {
            result.extend(wrap_styled(&format!("  {line}"), width, output_style));
        }
        let remaining = output_lines.len() - BASH_PREVIEW_LINES;
        result.push(Line::from(Span::styled(
            format!("  ... ({remaining} more lines)"),
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::DIM),
        )));
    } else {
        for line in output_lines {
            result.extend(wrap_styled(&format!("  {line}"), width, output_style));
        }
    }

    result
}

fn render_edit(content: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let all_lines: Vec<&str> = content.lines().collect();
    if all_lines.is_empty() {
        return vec![Line::from(Span::styled(
            "(no content)".to_string(),
            Style::default().fg(theme.text_muted),
        ))];
    }

    let mut result = Vec::new();

    let mut content_start = 0;
    if let Some(first) = all_lines.first() {
        let trimmed = first.trim();
        if looks_like_path(trimmed) {
            result.extend(wrap_styled(
                &format!("  {trimmed}"),
                width,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            content_start = 1;
        }
    }

    for line in &all_lines[content_start..] {
        let style = if line.starts_with('+') {
            Style::default()
                .fg(theme.diff_added)
                .bg(theme.diff_added_bg)
        } else if line.starts_with('-') {
            Style::default()
                .fg(theme.diff_removed)
                .bg(theme.diff_removed_bg)
        } else {
            Style::default().fg(theme.diff_context)
        };

        result.extend(wrap_styled(&format!("  {line}"), width, style));
    }

    result
}

fn render_read(content: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let all_lines: Vec<&str> = content.lines().collect();
    if all_lines.is_empty() {
        return vec![Line::from(Span::styled(
            "(no content)".to_string(),
            Style::default().fg(theme.text_muted),
        ))];
    }

    let mut result = Vec::new();

    let mut content_start = 0;
    if let Some(first) = all_lines.first() {
        let trimmed = first.trim();
        if looks_like_path(trimmed) {
            result.extend(wrap_styled(
                &format!("  \u{2192} {trimmed}"),
                width,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            content_start = 1;
        }
    }

    let remaining = &all_lines[content_start..];
    let content_style = Style::default().fg(theme.fg);

    if remaining.len() > READ_PREVIEW_LINES {
        for line in &remaining[..READ_PREVIEW_LINES] {
            result.extend(wrap_styled(&format!("  {line}"), width, content_style));
        }
        let more = remaining.len() - READ_PREVIEW_LINES;
        result.push(Line::from(Span::styled(
            format!("  ... ({more} more lines)"),
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::DIM),
        )));
    } else {
        for line in remaining {
            result.extend(wrap_styled(&format!("  {line}"), width, content_style));
        }
    }

    result
}

fn render_glob_grep(content: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let all_lines: Vec<&str> = content.lines().collect();
    if all_lines.is_empty() {
        return vec![Line::from(Span::styled(
            "(no matches)".to_string(),
            Style::default().fg(theme.text_muted),
        ))];
    }

    let mut result = Vec::new();

    let mut content_start = 0;
    if let Some(first) = all_lines.first() {
        let trimmed = first.trim();
        if is_header_line(trimmed) {
            result.extend(wrap_styled(
                &format!("  {trimmed}"),
                width,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            content_start = 1;
        }
    }

    let items = &all_lines[content_start..];
    let item_style = Style::default().fg(theme.fg);

    if items.len() > LIST_MAX_ITEMS {
        for line in &items[..LIST_MAX_ITEMS] {
            result.extend(wrap_styled(&format!("  {line}"), width, item_style));
        }
        let more = items.len() - LIST_MAX_ITEMS;
        result.push(Line::from(Span::styled(
            format!("  ... ({more} more items)"),
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::DIM),
        )));
    } else {
        for line in items {
            result.extend(wrap_styled(&format!("  {line}"), width, item_style));
        }
    }

    result
}

fn render_generic(content: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let all_lines: Vec<&str> = content.lines().collect();
    if all_lines.is_empty() {
        return vec![Line::from(Span::styled(
            "(no output)".to_string(),
            Style::default().fg(theme.text_muted),
        ))];
    }

    let content_style = Style::default().fg(theme.fg);
    let mut result = Vec::new();

    if all_lines.len() > GENERIC_PREVIEW_LINES {
        for line in &all_lines[..GENERIC_PREVIEW_LINES] {
            result.extend(wrap_styled(&format!("  {line}"), width, content_style));
        }
        let more = all_lines.len() - GENERIC_PREVIEW_LINES;
        result.push(Line::from(Span::styled(
            format!("  ... ({more} more lines)"),
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::DIM),
        )));
    } else {
        for line in &all_lines {
            result.extend(wrap_styled(&format!("  {line}"), width, content_style));
        }
    }

    result
}

fn looks_like_path(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.starts_with('/')
        || s.starts_with("./")
        || s.starts_with("../")
        || (s.contains('/') && !s.contains(' '))
        || s.ends_with(".rs")
        || s.ends_with(".ts")
        || s.ends_with(".tsx")
        || s.ends_with(".js")
        || s.ends_with(".jsx")
        || s.ends_with(".py")
        || s.ends_with(".go")
        || s.ends_with(".json")
        || s.ends_with(".toml")
        || s.ends_with(".yaml")
        || s.ends_with(".yml")
        || s.ends_with(".md")
}

fn is_header_line(s: &str) -> bool {
    s.starts_with("Pattern:")
        || s.starts_with("Searching")
        || s.starts_with("Found")
        || (!s.contains('/') && !looks_like_path(s))
}

fn wrap_styled(text: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![Line::from(Span::styled(text.to_string(), style))];
    }

    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![Line::from(Span::styled(String::new(), style))];
    }

    let mut lines = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let end = (i + width).min(chars.len());
        let chunk: String = chars[i..end].iter().collect();
        lines.push(Line::from(Span::styled(chunk, style)));
        i = end;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_theme() -> Theme {
        Theme::default()
    }

    #[test]
    fn test_bash_empty_output() {
        let lines = render_tool_output("Bash", "", 80, &test_theme());
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_bash_short_output() {
        let content = "ls -la\nfile1.rs\nfile2.rs";
        let lines = render_tool_output("bash", content, 80, &test_theme());
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_bash_long_output_truncated() {
        let mut content = String::from("cargo test");
        for i in 0..20 {
            content.push_str(&format!("\ntest line {i}"));
        }
        let lines = render_tool_output("Bash", &content, 80, &test_theme());
        assert_eq!(lines.len(), 12);
    }

    #[test]
    fn test_edit_with_diff_lines() {
        let content = "src/main.rs\n+added line\n-removed line\n context line";
        let lines = render_tool_output("Edit", content, 80, &test_theme());
        assert!(lines.len() >= 4);
    }

    #[test]
    fn test_read_with_path() {
        let content = "src/lib.rs\nuse std::io;\nfn main() {}";
        let lines = render_tool_output("Read", content, 80, &test_theme());
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_glob_results() {
        let content = "src/main.rs\nsrc/lib.rs\nsrc/utils.rs";
        let lines = render_tool_output("Glob", content, 80, &test_theme());
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_generic_fallback() {
        let content = "some output\nline 2";
        let lines = render_tool_output("UnknownTool", content, 80, &test_theme());
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_case_insensitive_dispatch() {
        let content = "ls\nfile.rs";
        let lines_lower = render_tool_output("bash", content, 80, &test_theme());
        let lines_upper = render_tool_output("Bash", content, 80, &test_theme());
        assert_eq!(lines_lower.len(), lines_upper.len());
    }

    #[test]
    fn test_looks_like_path() {
        assert!(looks_like_path("/usr/bin/test"));
        assert!(looks_like_path("./src/main.rs"));
        assert!(looks_like_path("src/lib.rs"));
        assert!(looks_like_path("file.json"));
        assert!(!looks_like_path(""));
        assert!(!looks_like_path("hello world"));
    }
}
