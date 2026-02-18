use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use serde_json::Value;

use crate::{
    widgets::diff::{render_diff, DiffProps, DiffView, WrapMode},
    Theme,
};

const BASH_MAX_LINES: usize = 15;
const BASH_PREVIEW_LINES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    Pending,
    Completed,
    Error,
    Denied,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolRenderContext {
    pub state: ToolState,
    pub expanded: bool,
    pub wide: bool,
    pub wrap_mode: WrapMode,
}

impl Default for ToolRenderContext {
    fn default() -> Self {
        Self {
            state: ToolState::Completed,
            expanded: true,
            wide: false,
            wrap_mode: WrapMode::Word,
        }
    }
}

pub fn render_tool_output(
    tool_name: &str,
    content: &str,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    let normalized = tool_name.to_lowercase();
    match normalized.as_str() {
        "bash" => render_bash(content, width, theme, context),
        "edit" | "write" => render_edit(content, width, theme, context),
        "read" => render_read(content, width, theme, context),
        "glob" | "grep" | "codesearch" | "code_search" => {
            render_code_search(content, width, theme, context)
        }
        "websearch" | "web_search" => render_web_search(content, width, theme, context),
        "webfetch" | "web_fetch" => render_web_fetch(content, width, theme, context),
        "applypatch" | "apply_patch" => render_apply_patch(content, width, theme, context),
        "question" | "ask_user" => render_question(content, width, theme, context),
        "task" | "delegate_task" | "subagent" => render_task(content, width, theme, context),
        _ => render_generic_inline(tool_name, content, width, theme, context),
    }
}

fn render_bash(
    content: &str,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    let mut lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return inline_tool("$", "Running bash...", None, width, theme, context);
    }

    let command = lines.remove(0).trim().to_string();
    let mut out = block_tool(
        "# Bash",
        vec![Line::from(Span::styled(
            format!("$ {command}"),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))],
        width,
        theme,
        context,
    );

    let show_truncated = lines.len() > BASH_MAX_LINES && !context.expanded;
    let visible = if show_truncated {
        &lines[..BASH_PREVIEW_LINES]
    } else {
        &lines[..]
    };

    for line in visible {
        out.extend(wrap_styled(
            &format!("  {line}"),
            width,
            Style::default().fg(theme.text_muted),
        ));
    }

    if show_truncated {
        out.push(Line::from(Span::styled(
            format!(
                "  ... ({} more lines)",
                lines.len().saturating_sub(BASH_PREVIEW_LINES)
            ),
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::DIM),
        )));
    }

    out
}

fn render_edit(
    content: &str,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    let mut all_lines: Vec<&str> = content.lines().collect();
    if all_lines.is_empty() {
        return inline_tool("←", "Preparing edit...", None, width, theme, context);
    }

    let mut file_path = None;
    if looks_like_path(all_lines[0].trim()) {
        file_path = Some(all_lines.remove(0).trim().to_string());
    }

    let diff = all_lines.join("\n");
    if diff.contains("@@") || diff.contains("\n+") || diff.contains("\n-") {
        let props = DiffProps {
            added_bg: theme.diff_added_bg,
            removed_bg: theme.diff_removed_bg,
            context_bg: theme.bg_panel,
            line_number_fg: theme.text_muted,
            view: if context.wide {
                DiffView::Split
            } else {
                DiffView::Unified
            },
            wrap_mode: context.wrap_mode,
        };
        let title = file_path
            .as_ref()
            .map(|p| format!("← Edit {p}"))
            .unwrap_or_else(|| "← Edit".to_string());
        let mut out = block_tool(&title, vec![], width, theme, context);
        out.extend(render_diff(
            &diff,
            file_path.as_deref(),
            width.saturating_sub(2),
            theme,
            props,
        ));
        return out;
    }

    let mut out = block_tool(
        file_path
            .as_ref()
            .map(|p| format!("# Wrote {p}"))
            .unwrap_or_else(|| "# Write".to_string()),
        vec![],
        width,
        theme,
        context,
    );
    for (idx, line) in diff.lines().enumerate() {
        out.push(line_numbered_content_line(idx + 1, line, width, theme));
    }
    out.extend(render_diagnostics(
        &extract_lsp_diagnostics(content),
        width,
        theme,
    ));
    out
}

fn render_read(
    content: &str,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    if content.trim().is_empty() {
        return inline_tool("→", "Reading file...", None, width, theme, context);
    }

    let mut lines = content.lines();
    let first = lines.next().unwrap_or_default();
    let complete = if looks_like_path(first.trim()) {
        Some(format!("Read {}", first.trim()))
    } else {
        Some("Read output".to_string())
    };

    inline_tool(
        "→",
        "Reading file...",
        complete.as_deref(),
        width,
        theme,
        context,
    )
}

fn render_code_search(
    content: &str,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    let (query, count) = extract_query_and_count(content);
    inline_tool(
        "◇",
        "Searching code...",
        Some(&format!(
            "Code search \"{}\" {}",
            query.unwrap_or_else(|| "...".to_string()),
            count.map(|n| format!("[{n} results]")).unwrap_or_default()
        )),
        width,
        theme,
        context,
    )
}

fn render_web_search(
    content: &str,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    let (query, count) = extract_query_and_count(content);
    inline_tool(
        "◈",
        "Searching web...",
        Some(&format!(
            "Web search \"{}\" {}",
            query.unwrap_or_else(|| "...".to_string()),
            count.map(|n| format!("[{n} results]")).unwrap_or_default()
        )),
        width,
        theme,
        context,
    )
}

fn render_web_fetch(
    content: &str,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    let url = extract_url(content).unwrap_or_else(|| "(unknown url)".to_string());
    let status = if context.state == ToolState::Error {
        "error"
    } else {
        "ok"
    };
    inline_tool(
        "%",
        "Fetching URL...",
        Some(&format!("WebFetch {url} [{status}]")),
        width,
        theme,
        context,
    )
}

fn render_apply_patch(
    content: &str,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    let parsed_json = serde_json::from_str::<Value>(content).ok();

    let Some(files) = parsed_json
        .as_ref()
        .and_then(|v| v.get("files"))
        .and_then(Value::as_array)
    else {
        return inline_tool(
            "%",
            "Preparing apply_patch...",
            Some("apply_patch"),
            width,
            theme,
            context,
        );
    };

    let mut out = Vec::new();
    for file in files {
        let file_type = file.get("type").and_then(Value::as_str).unwrap_or("patch");
        let relative_path = file
            .get("relativePath")
            .or_else(|| file.get("filePath"))
            .and_then(Value::as_str)
            .unwrap_or("(unknown)");
        let title = match file_type {
            "delete" => format!("# Deleted {relative_path}"),
            "add" => format!("# Created {relative_path}"),
            "move" => {
                let from = file
                    .get("filePath")
                    .and_then(Value::as_str)
                    .unwrap_or("(unknown)");
                format!("# Moved {from} -> {relative_path}")
            }
            _ => format!("# Patched {relative_path}"),
        };

        out.extend(block_tool(&title, vec![], width, theme, context));

        if file_type == "delete" {
            let deletions = file.get("deletions").and_then(Value::as_u64).unwrap_or(0);
            out.push(Line::from(Span::styled(
                format!("  -{deletions} lines"),
                Style::default().fg(theme.diff_removed),
            )));
            continue;
        }

        let diff_text = file.get("diff").and_then(Value::as_str).unwrap_or_default();
        let props = DiffProps {
            added_bg: theme.diff_added_bg,
            removed_bg: theme.diff_removed_bg,
            context_bg: theme.bg_panel,
            line_number_fg: theme.text_muted,
            view: if context.wide {
                DiffView::Split
            } else {
                DiffView::Unified
            },
            wrap_mode: context.wrap_mode,
        };
        out.extend(render_diff(
            diff_text,
            Some(relative_path),
            width.saturating_sub(2),
            theme,
            props,
        ));
    }

    out
}

fn render_question(
    content: &str,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    let parsed = serde_json::from_str::<Value>(content).ok();
    let mut lines = Vec::new();
    lines.extend(block_tool("# Questions", vec![], width, theme, context));

    if let Some(questions) = parsed
        .as_ref()
        .and_then(|v| v.get("questions"))
        .and_then(Value::as_array)
    {
        for q in questions {
            let question = q
                .get("question")
                .and_then(Value::as_str)
                .unwrap_or("(unknown question)");
            lines.extend(wrap_styled(
                &format!("  Q: {question}"),
                width,
                Style::default().fg(theme.text_muted),
            ));
            let answer = q
                .get("answer")
                .and_then(Value::as_str)
                .unwrap_or("(no answer)");
            lines.extend(wrap_styled(
                &format!("  A: {answer}"),
                width,
                Style::default().fg(theme.fg),
            ));
        }
    }

    if lines.len() <= 1 {
        lines.push(Line::from(Span::styled(
            "  (no question data)".to_string(),
            Style::default().fg(theme.text_muted),
        )));
    }

    lines
}

fn render_task(
    content: &str,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    let parsed = serde_json::from_str::<Value>(content).ok();
    let agent = parsed
        .as_ref()
        .and_then(|v| v.get("subagent_type"))
        .and_then(Value::as_str)
        .unwrap_or("agent");
    let description = parsed
        .as_ref()
        .and_then(|v| v.get("description"))
        .and_then(Value::as_str)
        .unwrap_or("delegated task");
    let count = parsed
        .as_ref()
        .and_then(|v| v.get("toolcalls"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let mut out = block_tool(
        format!("# {} task", title_case(agent)),
        vec![],
        width,
        theme,
        context,
    );
    out.push(Line::from(Span::styled(
        format!("  ◉ {description} ({count} toolcalls)"),
        Style::default().fg(theme.text_muted),
    )));
    out.push(Line::from(Span::styled(
        "  Hint: use child-session keybind to navigate subagent output".to_string(),
        Style::default().fg(theme.fg),
    )));
    out
}

fn render_generic_inline(
    tool_name: &str,
    content: &str,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    let preview = content
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|s| truncate_chars(s, 80))
        .unwrap_or_else(|| "(no output)".to_string());
    inline_tool(
        "⚙",
        "Running...",
        Some(&format!("{}: {}", tool_name, preview)),
        width,
        theme,
        context,
    )
}

fn inline_tool(
    icon: &str,
    pending: &str,
    complete: Option<&str>,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    let margin = if context.expanded { 1 } else { 0 };
    let mut out = vec![Line::from(""); margin];
    let denied = context.state == ToolState::Denied;
    let text = match context.state {
        ToolState::Pending => format!("~ {pending}"),
        _ => complete.unwrap_or(pending).to_string(),
    };
    let style = match context.state {
        ToolState::Pending => Style::default().fg(theme.fg),
        ToolState::Completed => Style::default().fg(theme.text_muted),
        ToolState::Error | ToolState::Denied => Style::default().fg(theme.error),
    }
    .add_modifier(if denied {
        Modifier::CROSSED_OUT
    } else {
        Modifier::empty()
    });

    out.extend(wrap_styled(&format!("   {icon} {text}"), width, style));
    out
}

fn block_tool(
    title: impl Into<String>,
    mut body: Vec<Line<'static>>,
    width: usize,
    theme: &Theme,
    context: ToolRenderContext,
) -> Vec<Line<'static>> {
    let title_style = if context.expanded {
        Style::default().fg(theme.text_muted)
    } else {
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::DIM)
    };
    let mut out = vec![Line::from(Span::styled(
        format!("  {}", title.into()),
        title_style,
    ))];
    out.append(&mut body);
    if context.state == ToolState::Error {
        out.push(Line::from(Span::styled(
            "  error".to_string(),
            Style::default().fg(theme.error),
        )));
    }
    if width > 0 {
        out.push(Line::from(""));
    }
    out
}

fn render_diagnostics(diagnostics: &[String], width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for diag in diagnostics {
        out.extend(wrap_styled(
            &format!("  Error: {diag}"),
            width,
            Style::default().fg(theme.error),
        ));
    }
    out
}

fn extract_lsp_diagnostics(content: &str) -> Vec<String> {
    let mut output = Vec::new();
    for line in content.lines() {
        let lower = line.to_lowercase();
        if lower.contains("error [") || lower.starts_with("error:") || lower.contains("diagnostic")
        {
            output.push(line.trim().to_string());
        }
    }
    output
}

fn extract_query_and_count(content: &str) -> (Option<String>, Option<usize>) {
    let parsed = serde_json::from_str::<Value>(content).ok();
    if let Some(value) = parsed {
        let query = value
            .get("query")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let count = value
            .get("numResults")
            .or_else(|| value.get("results"))
            .and_then(Value::as_u64)
            .map(|n| n as usize);
        return (query, count);
    }

    let query = content
        .lines()
        .find(|line| line.contains('"'))
        .and_then(|line| {
            let mut parts = line.split('"');
            parts.next()?;
            parts.next().map(ToString::to_string)
        });

    let count = content
        .split_whitespace()
        .find_map(|token| token.parse::<usize>().ok());

    (query, count)
}

fn extract_url(content: &str) -> Option<String> {
    let parsed = serde_json::from_str::<Value>(content).ok();
    if let Some(value) = parsed {
        if let Some(url) = value.get("url").and_then(Value::as_str) {
            return Some(url.to_string());
        }
    }
    content
        .split_whitespace()
        .find(|token| token.starts_with("https://") || token.starts_with("http://"))
        .map(ToString::to_string)
}

fn line_numbered_content_line(
    line_number: usize,
    line: &str,
    width: usize,
    theme: &Theme,
) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            format!("  {line_number:>4} "),
            Style::default().fg(theme.text_muted).bg(theme.bg_panel),
        ),
        Span::styled(line.to_string(), Style::default().fg(theme.fg)),
    ];
    if line.chars().count() > width {
        spans.push(Span::styled(
            " …",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::DIM),
        ));
    }
    Line::from(spans)
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

fn title_case(input: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in input.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            upper = true;
            out.push(' ');
            continue;
        }
        if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let mut output: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        output.push_str("...");
    }
    output
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
    fn renders_inline_pending() {
        let ctx = ToolRenderContext {
            state: ToolState::Pending,
            expanded: false,
            wide: false,
            wrap_mode: WrapMode::Word,
        };
        let lines = render_tool_output("bash", "", 80, &test_theme(), ctx);
        assert!(!lines.is_empty());
    }

    #[test]
    fn renders_apply_patch_json_files() {
        let content = r#"{"files":[{"type":"add","relativePath":"src/main.rs","filePath":"src/main.rs","diff":"@@ -0,0 +1,1 @@\n+fn main() {}"}]}"#;
        let ctx = ToolRenderContext {
            state: ToolState::Completed,
            expanded: true,
            wide: true,
            wrap_mode: WrapMode::Word,
        };
        let lines = render_tool_output("apply_patch", content, 120, &test_theme(), ctx);
        assert!(lines
            .iter()
            .any(|l| l.to_string().contains("Created src/main.rs")));
    }

    #[test]
    fn detects_denied_style() {
        let lines = inline_tool(
            "⚙",
            "pending",
            Some("done"),
            80,
            &test_theme(),
            ToolRenderContext {
                state: ToolState::Denied,
                expanded: false,
                wide: false,
                wrap_mode: WrapMode::Word,
            },
        );
        let has_strike = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.add_modifier.contains(Modifier::CROSSED_OUT));
        assert!(has_strike);
    }
}
