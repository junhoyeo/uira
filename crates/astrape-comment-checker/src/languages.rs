use std::collections::HashMap;
use std::path::Path;

use tree_sitter::Language;

pub struct LanguageRegistry {
    extension_map: HashMap<&'static str, &'static str>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        let mut extension_map = HashMap::new();

        // Core languages
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

        // Additional languages (Phase 1)
        extension_map.insert("cs", "csharp");
        extension_map.insert("php", "php");
        extension_map.insert("scala", "scala");
        extension_map.insert("sc", "scala");
        extension_map.insert("html", "html");
        extension_map.insert("htm", "html");
        extension_map.insert("css", "css");
        extension_map.insert("json", "json");
        extension_map.insert("hs", "haskell");
        extension_map.insert("ml", "ocaml");
        extension_map.insert("mli", "ocaml");

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
            "csharp" => Some(tree_sitter_c_sharp::LANGUAGE.into()),
            "php" => Some(tree_sitter_php::LANGUAGE_PHP.into()),
            "scala" => Some(tree_sitter_scala::LANGUAGE.into()),
            "html" => Some(tree_sitter_html::LANGUAGE.into()),
            "css" => Some(tree_sitter_css::LANGUAGE.into()),
            "json" => Some(tree_sitter_json::LANGUAGE.into()),
            "haskell" => Some(tree_sitter_haskell::LANGUAGE.into()),
            "ocaml" => Some(tree_sitter_ocaml::LANGUAGE_OCAML.into()),
            _ => None,
        }
    }

    pub fn is_supported(&self, file_path: &str) -> bool {
        self.get_language_name(file_path).is_some()
    }

    pub fn supported_extensions(&self) -> Vec<&'static str> {
        self.extension_map.keys().copied().collect()
    }

    pub fn supported_languages(&self) -> Vec<&'static str> {
        let mut langs: Vec<_> = self.extension_map.values().copied().collect();
        langs.sort();
        langs.dedup();
        langs
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
        "csharp" => "(comment) @comment",
        "php" => "(comment) @comment",
        "scala" => "(comment) @comment",
        "html" => "(comment) @comment",
        "css" => "(comment) @comment",
        "json" => "(comment) @comment",
        "haskell" => "(comment) @comment",
        "ocaml" => "(comment) @comment",
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
        assert_eq!(registry.get_language_name("test.cs"), Some("csharp"));
        assert_eq!(registry.get_language_name("test.php"), Some("php"));
        assert_eq!(registry.get_language_name("test.unknown"), None);
    }

    #[test]
    fn test_language_support() {
        let registry = LanguageRegistry::new();
        assert!(registry.is_supported("test.py"));
        assert!(registry.is_supported("test.js"));
        assert!(registry.is_supported("test.cs"));
        assert!(!registry.is_supported("test.unknown"));
    }

    #[test]
    fn test_supported_languages() {
        let registry = LanguageRegistry::new();
        let langs = registry.supported_languages();
        assert!(langs.contains(&"python"));
        assert!(langs.contains(&"rust"));
        assert!(langs.contains(&"csharp"));
        assert!(langs.contains(&"php"));
    }
}
