use xxhash_rust::xxh32::xxh32;

const NIBBLE_STR: &str = "ZPMQVRWSNKTXJBYH";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRef {
    pub line_number: usize,
    pub hash: [char; 2],
}

pub fn compute_line_hash(line_number: usize, line_text: &str) -> [char; 2] {
    let normalized = line_text
        .strip_suffix('\r')
        .unwrap_or(line_text)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();

    let seed = if normalized.chars().any(|c| c.is_alphanumeric()) {
        0
    } else {
        line_number as u32
    };
    let hash = xxh32(normalized.as_bytes(), seed);
    let index = (hash % 256) as usize;
    let high = index >> 4;
    let low = index & 0x0f;
    let bytes = NIBBLE_STR.as_bytes();
    [bytes[high] as char, bytes[low] as char]
}

pub fn compute_file_hash(content: &str) -> String {
    format!("{:08x}", xxh32(content.as_bytes(), 0))
}

pub fn line_tag(line_number: usize, line_text: &str) -> String {
    let [h1, h2] = compute_line_hash(line_number, line_text);
    format!("{}#{}{}", line_number, h1, h2)
}

pub fn parse_line_ref(value: &str) -> Option<LineRef> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if let Some((left, _)) = trimmed.split_once('|') {
        left.trim()
    } else {
        trimmed
    };

    let candidate = candidate.trim_start_matches('L');
    let (line_part, hash_part) = candidate.split_once('#')?;
    let line_number = line_part.trim().parse::<usize>().ok()?;
    if line_number == 0 {
        return None;
    }
    let mut chars = hash_part.trim().chars();
    let h1 = chars.next()?;
    let h2 = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    if !h1.is_ascii_alphabetic() || !h2.is_ascii_alphabetic() {
        return None;
    }

    Some(LineRef {
        line_number,
        hash: [h1, h2],
    })
}

pub fn parse_line_content(value: &str) -> String {
    if let Some((left, right)) = value.split_once(" | ") {
        if parse_line_ref(left).is_some() {
            return right.to_string();
        }
    }
    if let Some((left, right)) = value.split_once('|') {
        if parse_line_ref(left).is_some() {
            return right.to_string();
        }
    }
    value.to_string()
}

pub fn verify_line_ref(lines: &[String], line_ref: LineRef) -> Result<(), String> {
    if line_ref.line_number == 0 || line_ref.line_number > lines.len() {
        return Err(format!(
            "line {} is out of range (file has {} lines)",
            line_ref.line_number,
            lines.len()
        ));
    }

    let idx = line_ref.line_number - 1;
    let expected = line_ref.hash;
    let actual = compute_line_hash(line_ref.line_number, &lines[idx]);
    if actual != expected {
        return Err(format!(
            "hashline mismatch at {}#{}{} (actual {}#{})",
            line_ref.line_number,
            expected[0],
            expected[1],
            line_ref.line_number,
            actual.iter().collect::<String>()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_line_content_strips_only_valid_hashline_prefix() {
        let content = parse_line_content("12#PR | let x = a | b;");
        assert_eq!(content, "let x = a | b;");

        let raw = parse_line_content("let x = a | b;");
        assert_eq!(raw, "let x = a | b;");

        let indented = parse_line_content("2#AB |     return x");
        assert_eq!(indented, "    return x");

        let indented_no_delim_space = parse_line_content("2#AB|    return x");
        assert_eq!(indented_no_delim_space, "    return x");

        let zero_ref = parse_line_content("0#AB | should not strip");
        assert_eq!(zero_ref, "0#AB | should not strip");

        let zero_ref_no_delim_space = parse_line_content("0#AB|should not strip");
        assert_eq!(zero_ref_no_delim_space, "0#AB|should not strip");

        assert!(parse_line_ref("3#A1").is_none());
        assert!(parse_line_ref("0#AB").is_none());
    }
}
