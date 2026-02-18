use std::sync::OnceLock;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style as SyntectStyle, Theme as SyntectTheme, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

use crate::Theme;

const SPLIT_GUTTER: &str = " | ";
const HUNK_MARKER: &str = " @@ ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffView {
    Split,
    Unified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapMode {
    Word,
    None,
}

#[derive(Debug, Clone, Copy)]
pub struct DiffProps {
    pub added_bg: ratatui::style::Color,
    pub removed_bg: ratatui::style::Color,
    pub context_bg: ratatui::style::Color,
    pub line_number_fg: ratatui::style::Color,
    pub view: DiffView,
    pub wrap_mode: WrapMode,
}

impl DiffProps {
    #[cfg(test)]
    pub fn themed(theme: &Theme, width: usize) -> Self {
        Self {
            added_bg: theme.diff_added_bg,
            removed_bg: theme.diff_removed_bg,
            context_bg: theme.bg_panel,
            line_number_fg: theme.text_muted,
            view: if width > 120 {
                DiffView::Split
            } else {
                DiffView::Unified
            },
            wrap_mode: WrapMode::Word,
        }
    }
}

#[derive(Debug, Clone)]
struct DiffLine {
    kind: char,
    content: String,
    old_line: Option<usize>,
    new_line: Option<usize>,
}

pub fn render_diff(
    diff: &str,
    file_path: Option<&str>,
    width: usize,
    theme: &Theme,
    props: DiffProps,
) -> Vec<Line<'static>> {
    if diff.trim().is_empty() {
        return vec![Line::from(Span::styled(
            "  (no diff)".to_string(),
            Style::default().fg(theme.text_muted),
        ))];
    }

    let parsed = parse_unified_diff(diff);
    if parsed.is_empty() {
        return vec![Line::from(Span::styled(
            "  (unable to parse diff)".to_string(),
            Style::default().fg(theme.text_muted),
        ))];
    }

    match props.view {
        DiffView::Unified => render_unified(&parsed, file_path, width, theme, props),
        DiffView::Split => render_split(&parsed, file_path, width, theme, props),
    }
}

fn render_unified(
    lines: &[DiffLine],
    file_path: Option<&str>,
    width: usize,
    theme: &Theme,
    props: DiffProps,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let number_width = line_number_width(lines);

    for line in lines {
        if line.kind == '@' {
            out.push(Line::from(Span::styled(
                format!("  {}{}", HUNK_MARKER, line.content),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }

        let body = if file_path.is_some() {
            highlight_content(&line.content, file_path, theme)
        } else {
            vec![Span::styled(
                line.content.clone(),
                Style::default().fg(theme.fg),
            )]
        };

        let bg = background_for_kind(line.kind, props);
        let old = line
            .old_line
            .map(|n| format!("{n:>width$}", width = number_width))
            .unwrap_or_else(|| " ".repeat(number_width));
        let new = line
            .new_line
            .map(|n| format!("{n:>width$}", width = number_width))
            .unwrap_or_else(|| " ".repeat(number_width));

        let mut spans = vec![Span::styled(
            format!(" {old} {new} {} ", line.kind),
            Style::default().fg(props.line_number_fg).bg(bg),
        )];
        spans.extend(
            body.into_iter()
                .map(|s| Span::styled(s.content, s.style.bg(bg))),
        );

        let rendered = wrap_line(spans, width, props.wrap_mode);
        out.extend(rendered);
    }

    out
}

fn render_split(
    lines: &[DiffLine],
    file_path: Option<&str>,
    width: usize,
    theme: &Theme,
    props: DiffProps,
) -> Vec<Line<'static>> {
    if width < 40 {
        return render_unified(lines, file_path, width, theme, props);
    }

    let mut out = Vec::new();
    let number_width = line_number_width(lines);
    let column_width = width.saturating_sub(SPLIT_GUTTER.len()) / 2;

    let mut idx = 0;
    while idx < lines.len() {
        let line = &lines[idx];
        if line.kind == '@' {
            out.push(Line::from(Span::styled(
                format!("  {}{}", HUNK_MARKER, line.content),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )));
            idx += 1;
            continue;
        }

        if line.kind == '-' {
            let right_candidate = lines.get(idx + 1).filter(|l| l.kind == '+');
            let left_spans = render_split_side(
                line,
                file_path,
                number_width,
                column_width,
                theme,
                props,
                true,
            );
            let right_spans = right_candidate
                .map(|r| {
                    render_split_side(
                        r,
                        file_path,
                        number_width,
                        column_width,
                        theme,
                        props,
                        false,
                    )
                })
                .unwrap_or_else(|| blank_side(column_width, props.context_bg));

            out.push(join_split(left_spans, right_spans));
            idx += usize::from(right_candidate.is_some()) + 1;
            continue;
        }

        if line.kind == '+' {
            let left_spans = blank_side(column_width, props.context_bg);
            let right_spans = render_split_side(
                line,
                file_path,
                number_width,
                column_width,
                theme,
                props,
                false,
            );
            out.push(join_split(left_spans, right_spans));
            idx += 1;
            continue;
        }

        let left_spans = render_split_side(
            line,
            file_path,
            number_width,
            column_width,
            theme,
            props,
            true,
        );
        let right_spans = render_split_side(
            line,
            file_path,
            number_width,
            column_width,
            theme,
            props,
            false,
        );
        out.push(join_split(left_spans, right_spans));
        idx += 1;
    }

    out
}

fn render_split_side(
    line: &DiffLine,
    file_path: Option<&str>,
    number_width: usize,
    width: usize,
    theme: &Theme,
    props: DiffProps,
    left: bool,
) -> Vec<Span<'static>> {
    let line_no = if left { line.old_line } else { line.new_line };
    let number = line_no
        .map(|n| format!("{n:>width$}", width = number_width))
        .unwrap_or_else(|| " ".repeat(number_width));

    let kind = if (left && line.kind == '+') || (!left && line.kind == '-') {
        ' '
    } else {
        line.kind
    };

    let bg = background_for_kind(kind, props);
    let mut spans = vec![Span::styled(
        format!(" {number} {kind} "),
        Style::default().fg(props.line_number_fg).bg(bg),
    )];

    let highlighted = if file_path.is_some() {
        highlight_content(&line.content, file_path, theme)
    } else {
        vec![Span::styled(
            line.content.clone(),
            Style::default().fg(theme.fg),
        )]
    };
    spans.extend(
        highlighted
            .into_iter()
            .map(|s| Span::styled(s.content, s.style.bg(bg))),
    );

    fit_spans(spans, width, bg)
}

fn blank_side(width: usize, bg: ratatui::style::Color) -> Vec<Span<'static>> {
    vec![Span::styled(" ".repeat(width), Style::default().bg(bg))]
}

fn join_split(left: Vec<Span<'static>>, right: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = left;
    spans.push(Span::raw(SPLIT_GUTTER.to_string()));
    spans.extend(right);
    Line::from(spans)
}

fn fit_spans(
    spans: Vec<Span<'static>>,
    width: usize,
    bg: ratatui::style::Color,
) -> Vec<Span<'static>> {
    let mut rendered = Vec::new();
    let mut current_len = 0usize;

    for span in spans {
        let text = span.content.to_string();
        let len = text.chars().count();
        if current_len + len <= width {
            current_len += len;
            rendered.push(span);
            continue;
        }

        if current_len < width {
            let take = width - current_len;
            let trimmed: String = text.chars().take(take).collect();
            rendered.push(Span::styled(trimmed, span.style));
            current_len = width;
        }
        break;
    }

    if current_len < width {
        rendered.push(Span::styled(
            " ".repeat(width - current_len),
            Style::default().bg(bg),
        ));
    }

    rendered
}

fn background_for_kind(kind: char, props: DiffProps) -> ratatui::style::Color {
    match kind {
        '+' => props.added_bg,
        '-' => props.removed_bg,
        _ => props.context_bg,
    }
}

fn parse_unified_diff(diff: &str) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut old_line = 0usize;
    let mut new_line = 0usize;

    for raw in diff.lines() {
        if raw.starts_with("@@") {
            if let Some((old, new)) = parse_hunk_header(raw) {
                old_line = old;
                new_line = new;
            }
            lines.push(DiffLine {
                kind: '@',
                content: raw.to_string(),
                old_line: None,
                new_line: None,
            });
            continue;
        }

        if raw.starts_with("+++") || raw.starts_with("---") || raw.starts_with("diff --git") {
            continue;
        }

        let mut chars = raw.chars();
        let kind = chars.next().unwrap_or(' ');
        let content: String = chars.collect();

        match kind {
            '+' => {
                lines.push(DiffLine {
                    kind,
                    content,
                    old_line: None,
                    new_line: Some(new_line),
                });
                new_line += 1;
            }
            '-' => {
                lines.push(DiffLine {
                    kind,
                    content,
                    old_line: Some(old_line),
                    new_line: None,
                });
                old_line += 1;
            }
            ' ' => {
                lines.push(DiffLine {
                    kind,
                    content,
                    old_line: Some(old_line),
                    new_line: Some(new_line),
                });
                old_line += 1;
                new_line += 1;
            }
            _ => lines.push(DiffLine {
                kind: ' ',
                content: raw.to_string(),
                old_line: None,
                new_line: None,
            }),
        }
    }

    lines
}

fn parse_hunk_header(header: &str) -> Option<(usize, usize)> {
    let mut parts = header.split_whitespace();
    let _ = parts.next();
    let old = parts.next()?.trim_start_matches('-');
    let new = parts.next()?.trim_start_matches('+');

    let old_line = old.split(',').next()?.parse().ok()?;
    let new_line = new.split(',').next()?.parse().ok()?;
    Some((old_line, new_line))
}

fn line_number_width(lines: &[DiffLine]) -> usize {
    let max_old = lines.iter().filter_map(|l| l.old_line).max().unwrap_or(0);
    let max_new = lines.iter().filter_map(|l| l.new_line).max().unwrap_or(0);
    max_old.max(max_new).to_string().len().max(2)
}

fn wrap_line(spans: Vec<Span<'static>>, width: usize, wrap_mode: WrapMode) -> Vec<Line<'static>> {
    match wrap_mode {
        WrapMode::None => vec![Line::from(spans)],
        WrapMode::Word => {
            if width == 0 {
                return vec![Line::from(spans)];
            }

            let mut out = Vec::new();
            let mut buffer = String::new();
            let mut style = Style::default();

            for span in spans {
                style = span.style;
                buffer.push_str(span.content.as_ref());
            }

            if buffer.chars().count() <= width {
                out.push(Line::from(Span::styled(buffer, style)));
                return out;
            }

            let chars: Vec<char> = buffer.chars().collect();
            let mut idx = 0usize;
            while idx < chars.len() {
                let end = (idx + width).min(chars.len());
                let chunk: String = chars[idx..end].iter().collect();
                out.push(Line::from(Span::styled(chunk, style)));
                idx = end;
            }
            out
        }
    }
}

fn highlight_content(content: &str, file_path: Option<&str>, theme: &Theme) -> Vec<Span<'static>> {
    let Some(file_path) = file_path else {
        return vec![Span::styled(
            content.to_string(),
            Style::default().fg(theme.fg),
        )];
    };

    let extension = file_path
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or("txt");

    let syntax_set = syntax_set();
    let syntax = syntax_set
        .find_syntax_by_extension(extension)
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, syntect_theme());

    let mut spans = Vec::new();
    for line in LinesWithEndings::from(content) {
        if let Ok(regions) = highlighter.highlight_line(line, syntax_set) {
            spans.extend(regions.into_iter().map(|(style, text)| {
                Span::styled(text.to_string(), syntect_style_to_ratatui(style, theme))
            }));
        }
    }

    if spans.is_empty() {
        spans.push(Span::styled(
            content.to_string(),
            Style::default().fg(theme.fg),
        ));
    }
    spans
}

fn syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn syntect_theme() -> &'static SyntectTheme {
    static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();
    static THEME: OnceLock<SyntectTheme> = OnceLock::new();

    THEME.get_or_init(|| {
        let theme_set = THEME_SET.get_or_init(ThemeSet::load_defaults);
        theme_set
            .themes
            .get("base16-ocean.dark")
            .cloned()
            .or_else(|| theme_set.themes.values().next().cloned())
            .unwrap_or_default()
    })
}

fn syntect_style_to_ratatui(style: SyntectStyle, _theme: &Theme) -> Style {
    let mut out = Style::default().fg(ratatui::style::Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ));
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::BOLD)
    {
        out = out.add_modifier(Modifier::BOLD);
    }
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::ITALIC)
    {
        out = out.add_modifier(Modifier::ITALIC);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hunk_headers() {
        let parsed = parse_hunk_header("@@ -12,2 +34,3 @@");
        assert_eq!(parsed, Some((12, 34)));
    }

    #[test]
    fn parses_unified_diff_lines() {
        let diff = "@@ -1,2 +1,2 @@\n-a\n+b\n c";
        let lines = parse_unified_diff(diff);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[1].kind, '-');
        assert_eq!(lines[2].kind, '+');
    }

    #[test]
    fn renders_unified_non_empty() {
        let theme = Theme::default();
        let props = DiffProps::themed(&theme, 80);
        let lines = render_diff("@@ -1 +1 @@\n-a\n+b", Some("main.rs"), 80, &theme, props);
        assert!(!lines.is_empty());
    }
}
