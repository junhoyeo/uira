//! Standardized tool output formatting

/// Common section title constants
pub const SECTION_CONTENT: &str = "CONTENT";
pub const SECTION_DIFF: &str = "DIFF";
pub const SECTION_METADATA: &str = "METADATA";
pub const SECTION_ERROR: &str = "ERROR";

/// Trait for standardized tool output formatting
pub trait ToolOutputFormat {
    fn format_success(action: &str, details: &str) -> String;
    fn format_error(action: &str, error: &str) -> String;
    fn format_section(title: &str, content: &str) -> String;
    fn format_metadata(pairs: &[(&str, &str)]) -> String;
}

/// Standard implementation of tool output formatting
pub struct StandardOutput;

impl ToolOutputFormat for StandardOutput {
    fn format_success(action: &str, details: &str) -> String {
        format!("OK - {}\n{}", action, details)
    }

    fn format_error(action: &str, error: &str) -> String {
        format!("ERROR - {}\n{}", action, error)
    }

    fn format_section(title: &str, content: &str) -> String {
        format!("======== {} ========\n{}", title, content)
    }

    fn format_metadata(pairs: &[(&str, &str)]) -> String {
        if pairs.is_empty() {
            String::new()
        } else {
            pairs
                .iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>()
                .join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_success() {
        let result = StandardOutput::format_success("read", "file.txt");
        assert_eq!(result, "OK - read\nfile.txt");
    }

    #[test]
    fn test_format_error() {
        let result = StandardOutput::format_error("edit", "failed");
        assert_eq!(result, "ERROR - edit\nfailed");
    }

    #[test]
    fn test_format_section() {
        let result = StandardOutput::format_section("CONTENT", "hello world");
        assert_eq!(result, "======== CONTENT ========\nhello world");
    }

    #[test]
    fn test_format_metadata() {
        let pairs = vec![("key1", "value1"), ("key2", "value2")];
        let result = StandardOutput::format_metadata(&pairs);
        assert_eq!(result, "key1: value1\nkey2: value2");
    }

    #[test]
    fn test_format_metadata_empty() {
        let pairs: Vec<(&str, &str)> = vec![];
        let result = StandardOutput::format_metadata(&pairs);
        assert_eq!(result, "");
    }

    #[test]
    fn test_format_section_multiline() {
        let content = "line1\nline2\nline3";
        let result = StandardOutput::format_section("DIFF", content);
        assert_eq!(result, "======== DIFF ========\nline1\nline2\nline3");
    }
}
