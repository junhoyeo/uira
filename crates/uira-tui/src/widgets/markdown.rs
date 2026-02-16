use std::sync::OnceLock;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style as SyntectStyle, Theme as SyntectTheme, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

use crate::Theme;

struct LineBuilder {
    width: usize,
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    current_width: usize,
}

impl LineBuilder {
    fn new(width: usize) -> Self {
        Self {
            width: width.max(1),
            lines: Vec::new(),
            current: Vec::new(),
            current_width: 0,
        }
    }

    fn push_text(&mut self, text: &str, style: Style) {
        for ch in text.chars() {
            if ch == '\n' {
                self.finish_line();
                continue;
            }

            if self.current_width >= self.width {
                self.finish_line();
            }

            self.push_char(ch, style);
        }
    }

    fn push_char(&mut self, ch: char, style: Style) {
        if let Some(last) = self.current.last_mut() {
            if last.style == style {
                last.content.to_mut().push(ch);
            } else {
                self.current.push(Span::styled(ch.to_string(), style));
            }
        } else {
            self.current.push(Span::styled(ch.to_string(), style));
        }
        self.current_width += 1;
    }

    fn finish_line(&mut self) {
        if self.current.is_empty() {
            self.lines.push(Line::from(""));
        } else {
            let spans = std::mem::take(&mut self.current);
            self.lines.push(Line::from(spans));
        }
        self.current_width = 0;
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        if !self.current.is_empty() {
            let spans = std::mem::take(&mut self.current);
            self.lines.push(Line::from(spans));
        }

        if self.lines.is_empty() {
            self.lines.push(Line::from(""));
        }

        self.lines
    }
}

#[derive(Debug)]
struct CodeBlockState {
    language: Option<String>,
    content: String,
}

pub fn render_markdown(text: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut builder = LineBuilder::new(width);
    let base_style = Style::default().fg(theme.fg);
    let mut style_stack = vec![base_style];
    let options = Options::all();
    let parser = Parser::new_ext(text, options);
    let mut code_block: Option<CodeBlockState> = None;

    for event in parser {
        if let Some(state) = code_block.as_mut() {
            match event {
                Event::Text(content) | Event::Code(content) => state.content.push_str(&content),
                Event::SoftBreak | Event::HardBreak => state.content.push('\n'),
                Event::End(TagEnd::CodeBlock) => {
                    let finished = code_block.take();
                    if let Some(state) = finished {
                        let lines = render_code_block(state, width, theme);
                        for line in lines {
                            builder.finish_line();
                            if line.spans.is_empty() {
                                builder.push_text("", base_style);
                            } else {
                                for span in line.spans {
                                    builder.push_text(span.content.as_ref(), span.style);
                                }
                            }
                        }
                    }
                    builder.finish_line();
                }
                _ => {}
            }
            continue;
        }

        match event {
            Event::Start(tag) => match tag {
                Tag::Strong => {
                    let next = current_style(&style_stack).patch(
                        Style::default()
                            .fg(theme.md_strong)
                            .add_modifier(Modifier::BOLD),
                    );
                    style_stack.push(next);
                }
                Tag::Emphasis => {
                    let next = current_style(&style_stack).patch(
                        Style::default()
                            .fg(theme.md_emphasis)
                            .add_modifier(Modifier::ITALIC),
                    );
                    style_stack.push(next);
                }
                Tag::List(_) => {
                    // Start a list - we'll track list depth if needed
                    builder.finish_line();
                }
                Tag::Item => {
                    // Start a list item
                    builder.push_text("• ", current_style(&style_stack));
                }
                Tag::Heading { level, .. } => {
                    builder.finish_line();
                    let heading_style = match level {
                        pulldown_cmark::HeadingLevel::H1 => current_style(&style_stack).patch(
                            Style::default()
                                .fg(theme.md_strong)
                                .add_modifier(Modifier::BOLD)
                        ),
                        pulldown_cmark::HeadingLevel::H2 => current_style(&style_stack).patch(
                            Style::default()
                                .fg(theme.md_strong)
                                .add_modifier(Modifier::BOLD)
                        ),
                        _ => current_style(&style_stack).patch(
                            Style::default()
                                .add_modifier(Modifier::BOLD)
                        ),
                    };
                    style_stack.push(heading_style);
                    
                    // Add heading prefix
                    let prefix = "#".repeat(level as usize);
                    builder.push_text(&format!("{} ", prefix), heading_style);
                }
                Tag::Paragraph => {
                    // No special handling needed for paragraph start
                }
                Tag::BlockQuote(_) => {
                    builder.finish_line();
                    let quote_style = current_style(&style_stack).patch(
                        Style::default().fg(theme.borders)
                    );
                    style_stack.push(quote_style);
                }
                Tag::CodeBlock(kind) => {
                    let language = match kind {
                        CodeBlockKind::Fenced(info) => {
                            let trimmed = info.trim();
                            if trimmed.is_empty() {
                                None
                            } else {
                                let token = trimmed.split_whitespace().next().unwrap_or(trimmed);
                                Some(token.to_string())
                            }
                        }
                        CodeBlockKind::Indented => None,
                    };
                    code_block = Some(CodeBlockState {
                        language,
                        content: String::new(),
                    });
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Strong | TagEnd::Emphasis | TagEnd::Heading { .. } => {
                    if style_stack.len() > 1 {
                        style_stack.pop();
                    }
                }
                TagEnd::Paragraph => builder.finish_line(),
                TagEnd::Item => {
                    // Finish the current line for list item
                    builder.finish_line();
                }
                TagEnd::List(_) => {
                    // End of list - add spacing
                    builder.finish_line();
                }
                TagEnd::BlockQuote(_) => {
                    if style_stack.len() > 1 {
                        style_stack.pop();
                    }
                    builder.finish_line();
                }
                _ => {}
            },
            Event::Text(content) => builder.push_text(&content, current_style(&style_stack)),
            Event::Code(content) => {
                let inline = current_style(&style_stack)
                    .patch(Style::default().fg(theme.md_code_fg).bg(theme.md_code_bg));
                builder.push_text(&content, inline);
            }
            Event::SoftBreak | Event::HardBreak => builder.finish_line(),
            Event::Rule => builder.finish_line(),
            _ => {}
        }
    }

    builder.finish()
}

fn current_style(style_stack: &[Style]) -> Style {
    style_stack.last().copied().unwrap_or_default()
}

fn render_code_block(state: CodeBlockState, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    if let Some(language) = state.language {
        if !language.is_empty() {
            return highlight_code_block(&state.content, &language, width, theme);
        }
    }

    let mut builder = LineBuilder::new(width);
    let style = Style::default().fg(theme.md_code_fg).bg(theme.md_code_bg);
    builder.push_text(&state.content, style);
    builder.finish()
}

fn highlight_code_block(
    code: &str,
    language: &str,
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let syntax_set = syntax_set();
    let syntax = syntax_set
        .find_syntax_by_token(language)
        .or_else(|| syntax_set.find_syntax_by_extension(language))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, syntect_theme());
    let mut builder = LineBuilder::new(width);

    for line in LinesWithEndings::from(code) {
        if let Ok(regions) = highlighter.highlight_line(line, syntax_set) {
            for (style, text) in regions {
                builder.push_text(text, syntect_style_to_ratatui(style, theme));
            }
        } else {
            let fallback = Style::default().fg(theme.md_code_fg).bg(theme.md_code_bg);
            builder.push_text(line, fallback);
        }
    }

    builder.finish()
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

fn syntect_style_to_ratatui(style: SyntectStyle, theme: &Theme) -> Style {
    let mut out = Style::default()
        .fg(syntect_to_ratatui_color(style.foreground))
        .bg(theme.md_code_bg);

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
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::UNDERLINE)
    {
        out = out.add_modifier(Modifier::UNDERLINED);
    }

    out
}

fn syntect_to_ratatui_color(color: syntect::highlighting::Color) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_bold_modifier() {
        let theme = Theme::default();
        let lines = render_markdown("**bold**", 80, &theme);
        let has_bold = lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("bold") && span.style.add_modifier.contains(Modifier::BOLD)
        });
        assert!(has_bold);
    }

    #[test]
    fn renders_italic_modifier() {
        let theme = Theme::default();
        let lines = render_markdown("*italic*", 80, &theme);
        let has_italic = lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("italic") && span.style.add_modifier.contains(Modifier::ITALIC)
        });
        assert!(has_italic);
    }

    #[test]
    fn renders_inline_code_with_markdown_code_colors() {
        let theme = Theme::default();
        let lines = render_markdown("run `cargo test`", 80, &theme);
        let has_inline_code = lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("cargo test")
                && span.style.fg == Some(theme.md_code_fg)
                && span.style.bg == Some(theme.md_code_bg)
        });
        assert!(has_inline_code);
    }

    #[test]
    fn renders_fenced_code_block_without_language_with_code_background() {
        let theme = Theme::default();
        let lines = render_markdown("```\nlet x = 1;\n```", 80, &theme);
        let has_code_bg = lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("let x = 1;") && span.style.bg == Some(theme.md_code_bg)
        });
        assert!(has_code_bg);
    }

    #[test]
    fn renders_unordered_list_items() {
        let theme = Theme::default();
        let lines = render_markdown("- Item 1\n- Item 2\n- Item 3", 80, &theme);
        
        // Check that we have bullets
        let has_bullets = lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("•")
        });
        assert!(has_bullets);
        
        // Check that we have the content
        let has_items = lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("Item 1") || span.content.contains("Item 2") || span.content.contains("Item 3")
        });
        assert!(has_items);
    }
    
    #[test]
    fn renders_ordered_list_items() {
        let theme = Theme::default();
        let lines = render_markdown("1. First\n2. Second\n3. Third", 80, &theme);
        
        // Check that we have bullets (we use • for all lists for simplicity in TUI)
        let has_bullets = lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("•")
        });
        assert!(has_bullets);
    }
    
    #[test]
    fn renders_headings_with_hash_prefix() {
        let theme = Theme::default();
        let lines = render_markdown("# Heading 1\n## Heading 2", 80, &theme);
        
        let has_h1 = lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("# Heading 1") && span.style.add_modifier.contains(Modifier::BOLD)
        });
        assert!(has_h1);
        
        let has_h2 = lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("## Heading 2") && span.style.add_modifier.contains(Modifier::BOLD)
        });
        assert!(has_h2);
    }
    
    #[test]
    fn renders_blockquotes() {
        let theme = Theme::default();
        let lines = render_markdown("> This is a quote", 80, &theme);
        
        let has_quote_styling = lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("This is a quote") && span.style.fg == Some(theme.borders)
        });
        assert!(has_quote_styling);
    }

    #[test]
    fn renders_fenced_code_block_with_language_with_syntax_highlighting() {
        let theme = Theme::default();
        let lines = render_markdown("```rust\nlet x = 1;\n```", 80, &theme);

        let mut code_spans = lines
            .iter()
            .flat_map(|line| &line.spans)
            .filter(|span| !span.content.trim().is_empty())
            .collect::<Vec<_>>();

        assert!(!code_spans.is_empty());

        code_spans.retain(|span| span.style.bg == Some(theme.md_code_bg));
        assert!(!code_spans.is_empty());

        let has_non_default_fg = code_spans
            .iter()
            .any(|span| span.style.fg.is_some() && span.style.fg != Some(theme.md_code_fg));
        assert!(has_non_default_fg);
    }
}
