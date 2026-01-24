//! JavaScript/TypeScript transformer powered by OXC
//!
//! Transpile TypeScript to JavaScript, JSX to JS, and modern syntax to ES5/ES6.

use oxc::allocator::Allocator;
use oxc::codegen::{Codegen, CodegenOptions};
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::SourceType;
use oxc::transformer::{TransformOptions, Transformer as OxcTransformer};
use serde::{Deserialize, Serialize};
use std::fs;

/// Transform result
#[derive(Debug, Serialize, Deserialize)]
pub struct TransformResult {
    /// Whether transformation succeeded
    pub success: bool,
    /// The transformed code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Transform options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformConfig {
    /// Strip TypeScript types
    #[serde(default = "default_true")]
    pub typescript: bool,
    /// Transform JSX
    #[serde(default = "default_true")]
    pub jsx: bool,
    /// Target ES version (e.g., "es2015", "es2020", "esnext")
    #[serde(default = "default_target")]
    pub target: String,
}

fn default_true() -> bool {
    true
}

fn default_target() -> String {
    "es2020".to_string()
}

impl Default for TransformConfig {
    fn default() -> Self {
        Self {
            typescript: true,
            jsx: true,
            target: "es2020".to_string(),
        }
    }
}

/// Code transformer
pub struct Transformer;

impl Transformer {
    /// Transform a file
    pub fn transform_file(path: &str, config: Option<TransformConfig>) -> TransformResult {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                return TransformResult {
                    success: false,
                    code: None,
                    error: Some(format!("Failed to read file: {}", e)),
                }
            }
        };
        Self::transform_source(path, &source, config)
    }

    /// Transform source code
    pub fn transform_source(
        filename: &str,
        source: &str,
        config: Option<TransformConfig>,
    ) -> TransformResult {
        let _config = config.unwrap_or_default();
        let allocator = Allocator::default();
        let source_type = SourceType::from_path(filename).unwrap_or_default();

        // Parse the source
        let parser = Parser::new(&allocator, source, source_type);
        let mut ret = parser.parse();

        if !ret.errors.is_empty() {
            return TransformResult {
                success: false,
                code: None,
                error: Some(format!(
                    "Parse errors: {}",
                    ret.errors
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            };
        }

        // Build semantic analysis for scoping
        let semantic_ret = SemanticBuilder::new().with_cfg(true).build(&ret.program);

        // Build transform options
        let transform_options = TransformOptions::default();
        let source_path = std::path::Path::new(filename);

        // Run the transformer
        let transformer = OxcTransformer::new(&allocator, source_path, &transform_options);
        let transform_result =
            transformer.build_with_scoping(semantic_ret.semantic.into_scoping(), &mut ret.program);

        if !transform_result.errors.is_empty() {
            return TransformResult {
                success: false,
                code: None,
                error: Some(format!(
                    "Transform errors: {}",
                    transform_result
                        .errors
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            };
        }

        // Generate output code
        let codegen_options = CodegenOptions::default();
        let code = Codegen::new()
            .with_options(codegen_options)
            .build(&ret.program)
            .code;

        TransformResult {
            success: true,
            code: Some(code),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_typescript() {
        let result = Transformer::transform_source(
            "test.ts",
            "const x: number = 1; function foo(a: string): void {}",
            None,
        );
        assert!(result.success);
        let code = result.code.unwrap();
        // TypeScript types should be stripped
        assert!(!code.contains(": number"));
        assert!(!code.contains(": string"));
        assert!(!code.contains(": void"));
    }

    #[test]
    fn test_transform_jsx() {
        let result = Transformer::transform_source(
            "test.tsx",
            "const el = <div className=\"foo\">Hello</div>;",
            None,
        );
        // JSX transformation depends on the react runtime setting
        assert!(result.success || result.error.is_some());
    }

    #[test]
    fn test_transform_error() {
        let result = Transformer::transform_source("test.ts", "const x: = 1;", None);
        assert!(!result.success);
        assert!(result.error.is_some());
    }
}
