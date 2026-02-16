use uira_tui::widgets::markdown::render_markdown;
use uira_tui::Theme;

fn main() {
    let content = r#"# Test Markdown

## Lists
- First item
- Second item  
- Third item

1. Ordered item 1
2. Ordered item 2

## Formatting
**Bold text** and *italic text* and `inline code`.

> This is a blockquote
> With multiple lines

```rust
fn hello() {
    println!("Hello world!");
}
```
"#;

    let theme = Theme::default();
    let lines = render_markdown(content, 80, &theme);
    
    println!("Rendered {} lines of markdown:", lines.len());
    for (i, line) in lines.iter().enumerate() {
        println!("Line {}: {} spans", i, line.spans.len());
        for (j, span) in line.spans.iter().enumerate() {
            println!("  Span {}: '{}' (bold: {}, italic: {})", 
                j, 
                span.content, 
                span.style.add_modifier.contains(ratatui::style::Modifier::BOLD),
                span.style.add_modifier.contains(ratatui::style::Modifier::ITALIC)
            );
        }
    }
    
    // Test specific features
    let has_bullets = lines.iter()
        .flat_map(|line| &line.spans)
        .any(|span| span.content.contains("•"));
    println!("Lists have bullets: {}", has_bullets);
    
    let has_bold = lines.iter()
        .flat_map(|line| &line.spans)
        .any(|span| span.content.contains("Bold") && 
            span.style.add_modifier.contains(ratatui::style::Modifier::BOLD));
    println!("Bold formatting works: {}", has_bold);
    
    let has_italic = lines.iter()
        .flat_map(|line| &line.spans)
        .any(|span| span.content.contains("italic") && 
            span.style.add_modifier.contains(ratatui::style::Modifier::ITALIC));
    println!("Italic formatting works: {}", has_italic);
    
    let has_code_bg = lines.iter()
        .flat_map(|line| &line.spans)
        .any(|span| span.content.contains("inline code") && 
            span.style.bg.is_some());
    println!("Inline code has background: {}", has_code_bg);
}