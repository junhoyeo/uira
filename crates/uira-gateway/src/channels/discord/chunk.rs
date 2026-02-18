const DEFAULT_MAX_CHARS: usize = 2000;
const DEFAULT_MAX_LINES: usize = 17;

#[derive(Debug, Clone)]
pub struct ChunkOpts {
    pub max_chars: usize,
    pub max_lines: usize,
}

impl Default for ChunkOpts {
    fn default() -> Self {
        Self {
            max_chars: DEFAULT_MAX_CHARS,
            max_lines: DEFAULT_MAX_LINES,
        }
    }
}

#[derive(Debug, Clone)]
struct OpenFence {
    indent: String,
    marker_char: char,
    marker_len: usize,
    open_line: String,
}

fn count_lines(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    text.split('\n').count()
}

fn parse_fence_line(line: &str) -> Option<OpenFence> {
    // Match: up to 3 spaces of indent, then 3+ backticks or tildes, then anything
    let bytes = line.as_bytes();
    let mut pos = 0;

    // Count leading spaces (up to 3)
    while pos < bytes.len() && pos < 3 && bytes[pos] == b' ' {
        pos += 1;
    }
    let indent = &line[..pos];

    if pos >= bytes.len() {
        return None;
    }

    let marker_char = bytes[pos] as char;
    if marker_char != '`' && marker_char != '~' {
        return None;
    }

    let marker_start = pos;
    while pos < bytes.len() && bytes[pos] as char == marker_char {
        pos += 1;
    }
    let marker_len = pos - marker_start;
    if marker_len < 3 {
        return None;
    }

    Some(OpenFence {
        indent: indent.to_string(),
        marker_char,
        marker_len,
        open_line: line.to_string(),
    })
}

fn close_fence_line(fence: &OpenFence) -> String {
    format!(
        "{}{}",
        fence.indent,
        fence.marker_char.to_string().repeat(fence.marker_len)
    )
}

fn close_fence_if_needed(text: &str, fence: Option<&OpenFence>) -> String {
    let Some(fence) = fence else {
        return text.to_string();
    };
    let close = close_fence_line(fence);
    if text.is_empty() {
        return close;
    }
    if text.ends_with('\n') {
        format!("{text}{close}")
    } else {
        format!("{text}\n{close}")
    }
}

fn char_safe_limit(s: &str, byte_limit: usize) -> usize {
    let capped = byte_limit.min(s.len());
    let mut idx = capped;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    if idx == 0 && capped > 0 {
        s.char_indices().nth(1).map(|(i, _)| i).unwrap_or(s.len())
    } else {
        idx
    }
}

fn split_long_line(line: &str, max_chars: usize, preserve_whitespace: bool) -> Vec<String> {
    let limit = max_chars.max(1);
    if line.len() <= limit {
        return vec![line.to_string()];
    }
    let mut out = Vec::new();
    let mut remaining = line;
    while remaining.len() > limit {
        let byte_limit = char_safe_limit(remaining, limit);
        if preserve_whitespace {
            out.push(remaining[..byte_limit].to_string());
            remaining = &remaining[byte_limit..];
            continue;
        }
        let window = &remaining[..byte_limit];
        let mut break_idx = None;
        for (i, c) in window.char_indices().rev() {
            if c.is_whitespace() {
                break_idx = Some(i);
                break;
            }
        }
        let idx = break_idx.unwrap_or(byte_limit);
        let idx = if idx == 0 { byte_limit } else { idx };
        out.push(remaining[..idx].to_string());
        remaining = &remaining[idx..];
    }
    if !remaining.is_empty() {
        out.push(remaining.to_string());
    }
    out
}

pub fn chunk_discord_text(text: &str, opts: &ChunkOpts) -> Vec<String> {
    let max_chars = opts.max_chars.max(1);
    let max_lines = opts.max_lines.max(1);

    if text.is_empty() {
        return Vec::new();
    }

    if text.len() <= max_chars && count_lines(text) <= max_lines {
        return vec![text.to_string()];
    }

    let lines: Vec<&str> = text.split('\n').collect();
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_lines: usize = 0;
    let mut open_fence: Option<OpenFence> = None;

    let flush = |current: &mut String,
                 current_lines: &mut usize,
                 open_fence: &Option<OpenFence>,
                 chunks: &mut Vec<String>| {
        if current.is_empty() {
            return;
        }
        let payload = close_fence_if_needed(current, open_fence.as_ref());
        if !payload.trim().is_empty() {
            chunks.push(payload);
        }
        current.clear();
        *current_lines = 0;
        if let Some(fence) = open_fence {
            *current = fence.open_line.clone();
            *current_lines = 1;
        }
    };

    for original_line in &lines {
        let fence_info = parse_fence_line(original_line);
        let was_inside_fence = open_fence.is_some();
        let mut next_open_fence = open_fence.clone();

        if let Some(ref fi) = fence_info {
            if let Some(ref of_) = open_fence {
                if of_.marker_char == fi.marker_char && fi.marker_len >= of_.marker_len {
                    next_open_fence = None;
                }
            } else {
                next_open_fence = Some(fi.clone());
            }
        }

        let reserve_chars = next_open_fence
            .as_ref()
            .map(|f| close_fence_line(f).len() + 1)
            .unwrap_or(0);
        let reserve_lines: usize = if next_open_fence.is_some() { 1 } else { 0 };
        let effective_max_chars = if max_chars > reserve_chars {
            max_chars - reserve_chars
        } else {
            max_chars
        };
        let effective_max_lines = if max_lines > reserve_lines {
            max_lines - reserve_lines
        } else {
            max_lines
        };

        let prefix_len = if current.is_empty() {
            0
        } else {
            current.len() + 1
        };
        let segment_limit = if effective_max_chars > prefix_len {
            effective_max_chars - prefix_len
        } else {
            1
        };

        let segments = split_long_line(original_line, segment_limit, was_inside_fence);

        for (seg_index, segment) in segments.iter().enumerate() {
            let is_continuation = seg_index > 0;
            let delimiter = if !is_continuation && !current.is_empty() {
                "\n"
            } else {
                ""
            };

            let addition_len = delimiter.len() + segment.len();
            let next_len = current.len() + addition_len;
            let next_lines = current_lines + if is_continuation { 0 } else { 1 };

            let would_exceed_chars = next_len > effective_max_chars;
            let would_exceed_lines = next_lines > effective_max_lines;

            if (would_exceed_chars || would_exceed_lines) && !current.is_empty() {
                flush(&mut current, &mut current_lines, &open_fence, &mut chunks);
            }

            if current.is_empty() {
                current = segment.clone();
                current_lines = 1;
            } else {
                current.push_str(delimiter);
                current.push_str(segment);
                if !is_continuation {
                    current_lines += 1;
                }
            }
        }

        open_fence = next_open_fence;
    }

    if !current.is_empty() {
        let payload = close_fence_if_needed(&current, open_fence.as_ref());
        if !payload.trim().is_empty() {
            chunks.push(payload);
        }
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_text() {
        assert!(chunk_discord_text("", &ChunkOpts::default()).is_empty());
    }

    #[test]
    fn test_short_text_returns_single_chunk() {
        let text = "Hello, world!";
        let chunks = chunk_discord_text(text, &ChunkOpts::default());
        assert_eq!(chunks, vec!["Hello, world!"]);
    }

    #[test]
    fn test_long_text_splits_by_chars() {
        let text = "a".repeat(4500);
        let opts = ChunkOpts {
            max_chars: 2000,
            max_lines: 100,
        };
        let chunks = chunk_discord_text(&text, &opts);
        assert!(chunks.len() >= 3);
        for chunk in &chunks {
            assert!(chunk.len() <= 2000);
        }
    }

    #[test]
    fn test_many_lines_splits_by_line_count() {
        let lines: Vec<&str> = (0..40).map(|_| "short line").collect();
        let text = lines.join("\n");
        let opts = ChunkOpts {
            max_chars: 10000,
            max_lines: 17,
        };
        let chunks = chunk_discord_text(&text, &opts);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn test_code_fence_balanced() {
        let text = "```python\nprint('hello')\nprint('world')\n```";
        let opts = ChunkOpts {
            max_chars: 30,
            max_lines: 100,
        };
        let chunks = chunk_discord_text(text, &opts);
        for chunk in &chunks {
            let opens = chunk.matches("```").count();
            assert_eq!(opens % 2, 0, "Unbalanced fences in chunk: {chunk}");
        }
    }

    #[test]
    fn test_code_fence_split_reopens() {
        let mut lines = vec!["```rust"];
        for _ in 0..30 {
            lines.push("let x = 1;");
        }
        lines.push("```");
        let text = lines.join("\n");
        let opts = ChunkOpts {
            max_chars: 200,
            max_lines: 10,
        };
        let chunks = chunk_discord_text(&text, &opts);
        assert!(chunks.len() >= 2);
        assert!(chunks[0].contains("```"));
        for chunk in &chunks[1..] {
            assert!(chunk.starts_with("```rust"));
        }
    }

    #[test]
    fn test_multibyte_utf8_does_not_panic() {
        let text = "🔥".repeat(1500);
        let opts = ChunkOpts {
            max_chars: 2000,
            max_lines: 100,
        };
        let chunks = chunk_discord_text(&text, &opts);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(chunk.len() <= 2000);
        }
        let reassembled: String = chunks.concat();
        assert_eq!(reassembled, text);
    }

    #[test]
    fn test_mixed_ascii_and_multibyte() {
        let text = "Hello 🌍 World 🚀 ".repeat(200);
        let opts = ChunkOpts {
            max_chars: 100,
            max_lines: 100,
        };
        let chunks = chunk_discord_text(&text, &opts);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(chunk.len() <= 100);
        }
    }

    #[test]
    fn test_tilde_fence() {
        let text = "~~~\ncode here\n~~~";
        let opts = ChunkOpts {
            max_chars: 10,
            max_lines: 100,
        };
        let chunks = chunk_discord_text(text, &opts);
        assert!(!chunks.is_empty());
    }
}
