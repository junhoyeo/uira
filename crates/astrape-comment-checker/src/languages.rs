use std::collections::HashMap;
use std::path::Path;

use tree_sitter::Language;

pub struct LanguageRegistry {
    extension_map: HashMap<&'static str, &'static str>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        let mut extension_map = HashMap::new();

        extension_map.insert("py", "python");
        extension_map.insert("js", "javascript");
        extension_map.insert("jsx", "javascript");
        extension_map.insert("ts", "typescript");
        extension_map.insert("tsx", "tsx");
        extension_map.insert("go", "go");
        extension_map.insert("java", "java");
        extension_map.insert("c", "c");
        extension_map.insert("h", "c");
        extension_map.insert("cpp", "cpp");
        extension_map.insert("cc", "cpp");
        extension_map.insert("cxx", "cpp");
        extension_map.insert("hpp", "cpp");
        extension_map.insert("rs", "rust");
        extension_map.insert("rb", "ruby");
        extension_map.insert("sh", "bash");
        extension_map.insert("bash", "bash");

        Self { extension_map }
    }

    pub fn get_language_name(&self, file_path: &str) -> Option<&'static str> {
        let ext = Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_else(|| {
                Path::new(file_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
            });

        self.extension_map.get(ext.to_lowercase().as_str()).copied()
    }

    pub fn get_language(&self, name: &str) -> Option<Language> {
        match name {
            "python" => Some(tree_sitter_python::LANGUAGE.into()),
            "javascript" => Some(tree_sitter_javascript::LANGUAGE.into()),
            "typescript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
            "go" => Some(tree_sitter_go::LANGUAGE.into()),
            "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
            "c" => Some(tree_sitter_c::LANGUAGE.into()),
            "cpp" => Some(tree_sitter_cpp::LANGUAGE.into()),
            "java" => Some(tree_sitter_java::LANGUAGE.into()),
            "ruby" => Some(tree_sitter_ruby::LANGUAGE.into()),
            "bash" => Some(tree_sitter_bash::LANGUAGE.into()),
            _ => None,
        }
    }

    pub fn is_supported(&self, file_path: &str) -> bool {
        self.get_language_name(file_path).is_some()
    }
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn get_query_pattern(lang_name: &str) -> &'static str {
    match lang_name {
        "rust" => "(line_comment) @comment (block_comment) @comment",
        "python" => "(comment) @comment",
        "javascript" | "typescript" | "tsx" => "(comment) @comment",
        "go" => "(comment) @comment",
        "c" | "cpp" => "(comment) @comment",
        "java" => "(comment) @comment (block_comment) @comment (line_comment) @comment",
        "ruby" => "(comment) @comment",
        "bash" => "(comment) @comment",
        _ => "(comment) @comment",
    }
}

pub fn get_comment_query(lang_name: &str, language: Language) -> Option<tree_sitter::Query> {
    let pattern = get_query_pattern(lang_name);
    tree_sitter::Query::new(&language, pattern).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_mapping() {
        let registry = LanguageRegistry::new();
        assert_eq!(registry.get_language_name("test.py"), Some("python"));
        assert_eq!(registry.get_language_name("test.js"), Some("javascript"));
        assert_eq!(registry.get_language_name("test.rs"), Some("rust"));
        assert_eq!(registry.get_language_name("test.unknown"), None);
    }

    #[test]
    fn test_language_support() {
        let registry = LanguageRegistry::new();
        assert!(registry.is_supported("test.py"));
        assert!(registry.is_supported("test.js"));
        assert!(!registry.is_supported("test.unknown"));
    }
}
