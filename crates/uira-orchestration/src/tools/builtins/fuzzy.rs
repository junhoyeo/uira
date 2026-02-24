//! Levenshtein fuzzy matching for finding similar strings in content.
#![allow(dead_code)]
//!
//! This module provides utilities for finding similar strings when exact matches fail,
//! useful for suggesting corrections or alternatives.

/// Calculate the Levenshtein distance between two strings.
///
/// Uses a space-optimized dynamic programming algorithm with O(min(m,n)) space complexity.
/// Handles Unicode correctly by operating on characters rather than bytes.
///
/// # Arguments
/// * `a` - First string
/// * `b` - Second string
///
/// # Returns
/// The minimum number of single-character edits (insertions, deletions, substitutions)
/// required to change `a` into `b`.
///
/// # Examples
/// ```ignore
/// assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
/// assert_eq!(levenshtein_distance("hello", "hello"), 0);
/// ```
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    // Use two rows for space optimization
    let mut prev_row = vec![0; b_len + 1];
    let mut curr_row = vec![0; b_len + 1];

    // Initialize first row
    for (j, item) in prev_row.iter_mut().enumerate().take(b_len + 1) {
        *item = j;
    }

    // Fill the DP table
    for i in 1..=a_len {
        curr_row[0] = i;

        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };

            curr_row[j] = std::cmp::min(
                std::cmp::min(
                    prev_row[j] + 1,     // deletion
                    curr_row[j - 1] + 1, // insertion
                ),
                prev_row[j - 1] + cost, // substitution
            );
        }

        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[b_len]
}

/// Calculate the similarity ratio between two strings.
///
/// Returns a value between 0.0 and 1.0, where 1.0 means identical strings.
///
/// # Arguments
/// * `a` - First string
/// * `b` - Second string
///
/// # Returns
/// Similarity ratio: `1.0 - (distance / max_length)`
///
/// # Examples
/// ```ignore
/// assert_eq!(similarity_ratio("hello", "hello"), 1.0);
/// assert!(similarity_ratio("hello", "hallo") > 0.8);
/// ```
pub fn similarity_ratio(a: &str, b: &str) -> f64 {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    // Both empty strings are identical
    if a_len == 0 && b_len == 0 {
        return 1.0;
    }

    let max_len = std::cmp::max(a_len, b_len);
    if max_len == 0 {
        return 1.0;
    }

    let distance = levenshtein_distance(a, b);
    1.0 - (distance as f64 / max_len as f64)
}

/// A string match with similarity score and context.
#[derive(Debug, Clone)]
pub struct SimilarString {
    /// The matched text
    pub text: String,
    /// 1-based line number
    pub line_number: usize,
    /// Similarity ratio (0.0-1.0)
    pub similarity: f64,
    /// Lines before the match
    pub context_before: Vec<String>,
    /// Lines after the match
    pub context_after: Vec<String>,
}

/// Options for finding similar strings.
#[derive(Debug, Clone)]
pub struct FindOptions {
    /// Maximum number of results to return
    pub max_results: usize,
    /// Minimum similarity threshold (0.0-1.0)
    pub min_similarity: f64,
    /// Number of context lines to include before/after
    pub context_lines: usize,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            max_results: 5,
            min_similarity: 0.4,
            context_lines: 2,
        }
    }
}

/// Find similar strings in content.
///
/// Searches for strings similar to `search` in the given content.
/// For each line, checks both whole-line similarity and substring matches
/// using a sliding window approach.
///
/// # Arguments
/// * `content` - The content to search in
/// * `search` - The string to search for
/// * `options` - Search options (max_results, min_similarity, context_lines)
///
/// # Returns
/// Vector of similar strings sorted by similarity (highest first)
pub fn find_similar_strings(
    content: &str,
    search: &str,
    options: &FindOptions,
) -> Vec<SimilarString> {
    let lines: Vec<&str> = content.lines().collect();
    let search_len = search.chars().count();
    let window_min = if search_len > 5 { search_len - 5 } else { 1 };
    let window_max = search_len + 5;

    let mut matches: Vec<(usize, f64, String)> = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        let line_num = line_idx + 1;

        // Check whole-line similarity
        let whole_line_similarity = similarity_ratio(line, search);

        // Check substring matches with sliding window
        let mut best_substring_similarity = 0.0;
        let line_chars: Vec<char> = line.chars().collect();

        for window_size in window_min..=window_max {
            if window_size > line_chars.len() {
                continue;
            }

            for start in 0..=(line_chars.len() - window_size) {
                let substring: String = line_chars[start..start + window_size].iter().collect();
                let substring_similarity = similarity_ratio(&substring, search);

                if substring_similarity > best_substring_similarity {
                    best_substring_similarity = substring_similarity;
                }
            }
        }

        // Take the better of whole-line or best-substring similarity
        let best_similarity = whole_line_similarity.max(best_substring_similarity);

        if best_similarity >= options.min_similarity {
            matches.push((line_num, best_similarity, line.to_string()));
        }
    }

    // Sort by similarity descending
    matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Truncate to max_results
    matches.truncate(options.max_results);

    // Build results with context
    matches
        .into_iter()
        .map(|(line_num, similarity, text)| {
            let line_idx = line_num - 1;

            let context_before = if line_idx > 0 {
                let start = line_idx.saturating_sub(options.context_lines);
                lines[start..line_idx]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            } else {
                Vec::new()
            };

            let context_after = if line_idx < lines.len() - 1 {
                let end = std::cmp::min(line_idx + options.context_lines + 1, lines.len());
                lines[line_idx + 1..end]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            } else {
                Vec::new()
            };

            SimilarString {
                text,
                line_number: line_num,
                similarity,
                context_before,
                context_after,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_basic() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein_distance("", "hello"), 5);
        assert_eq!(levenshtein_distance("hello", ""), 5);
        assert_eq!(levenshtein_distance("", ""), 0);
    }

    #[test]
    fn test_levenshtein_unicode() {
        // Test with Unicode characters
        assert_eq!(levenshtein_distance("café", "cafe"), 1);
        assert_eq!(levenshtein_distance("🦀", "🦀"), 0);
    }

    #[test]
    fn test_similarity_ratio_identical() {
        assert_eq!(similarity_ratio("hello", "hello"), 1.0);
    }

    #[test]
    fn test_similarity_ratio_different() {
        let ratio = similarity_ratio("hello", "hallo");
        assert!(ratio >= 0.8 && ratio < 1.0);
    }

    #[test]
    fn test_similarity_ratio_empty_both() {
        assert_eq!(similarity_ratio("", ""), 1.0);
    }

    #[test]
    fn test_similarity_ratio_one_empty() {
        let ratio = similarity_ratio("hello", "");
        assert!(ratio < 0.5);
    }

    #[test]
    fn test_find_similar_strings_basic() {
        let content = "line one\nline two\nline three\n";
        let results = find_similar_strings(content, "line two", &FindOptions::default());

        assert!(!results.is_empty());
        assert_eq!(results[0].text, "line two");
        assert_eq!(results[0].line_number, 2);
        assert_eq!(results[0].similarity, 1.0);
    }

    #[test]
    fn test_find_similar_strings_sorted() {
        let content = "apple\napple pie\napple juice\nbanana\n";
        let results = find_similar_strings(content, "apple", &FindOptions::default());

        // Results should be sorted by similarity descending
        for i in 0..results.len() - 1 {
            assert!(results[i].similarity >= results[i + 1].similarity);
        }
    }

    #[test]
    fn test_find_similar_strings_with_context() {
        let content = "line 1\nline 2\nline 3\nline 4\nline 5\n";
        let options = FindOptions {
            context_lines: 2,
            ..Default::default()
        };
        let results = find_similar_strings(content, "line 3", &options);

        assert!(!results.is_empty());
        let match_result = &results[0];
        assert_eq!(match_result.line_number, 3);
        assert_eq!(match_result.context_before.len(), 2);
        assert_eq!(match_result.context_after.len(), 2);
    }

    #[test]
    fn test_find_similar_strings_no_match() {
        let content = "apple\nbanana\ncherry\n";
        let options = FindOptions {
            min_similarity: 0.9,
            ..Default::default()
        };
        let results = find_similar_strings(content, "xyz", &options);

        assert!(results.is_empty());
    }

    #[test]
    fn test_find_similar_strings_substring_window() {
        let content = "the quick brown fox jumps\nover the lazy dog\n";
        let results = find_similar_strings(content, "quick", &FindOptions::default());

        // Should find "quick" as a substring in the first line
        assert!(!results.is_empty());
        assert!(results[0].similarity > 0.5);
    }

    #[test]
    fn test_find_similar_strings_max_results() {
        let content = "apple\napple pie\napple juice\napple sauce\napple cider\napple tart\n";
        let options = FindOptions {
            max_results: 2,
            ..Default::default()
        };
        let results = find_similar_strings(content, "apple", &options);

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_find_similar_strings_min_similarity() {
        let content = "apple\nbanana\ncherry\napricot\n";
        let options = FindOptions {
            min_similarity: 0.7,
            ..Default::default()
        };
        let results = find_similar_strings(content, "apple", &options);

        // All results should meet the minimum similarity threshold
        for result in &results {
            assert!(result.similarity >= 0.7);
        }
    }

    #[test]
    fn test_find_similar_strings_edge_case_single_line() {
        let content = "single line";
        let results = find_similar_strings(content, "single", &FindOptions::default());

        assert!(!results.is_empty());
        assert_eq!(results[0].context_before.len(), 0);
        assert_eq!(results[0].context_after.len(), 0);
    }
}
