//! JavaScript minifier powered by OXC
//!
//! Fast minification of JavaScript code.

use oxc::allocator::Allocator;
use oxc::codegen::{Codegen, CodegenOptions};
use oxc::minifier::{CompressOptions, MangleOptions, Minifier as OxcMinifier, MinifierOptions};
use oxc::parser::Parser;
use oxc::span::SourceType;
use serde::{Deserialize, Serialize};
use std::fs;

/// Minification result
#[derive(Debug, Serialize, Deserialize)]
pub struct MinifyResult {
    /// Whether minification succeeded
    pub success: bool,
    /// The minified code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Original size in bytes
    pub original_size: usize,
    /// Minified size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minified_size: Option<usize>,
    /// Compression ratio
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression_ratio: Option<f64>,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Minification options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinifyConfig {
    /// Mangle variable names
    #[serde(default = "default_true")]
    pub mangle: bool,
    /// Compress/optimize the code
    #[serde(default = "default_true")]
    pub compress: bool,
}

fn default_true() -> bool {
    true
}

impl Default for MinifyConfig {
    fn default() -> Self {
        Self {
            mangle: true,
            compress: true,
        }
    }
}

/// JavaScript minifier
pub struct Minifier;

impl Minifier {
    /// Minify a file
    pub fn minify_file(path: &str, config: Option<MinifyConfig>) -> MinifyResult {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                return MinifyResult {
                    success: false,
                    code: None,
                    original_size: 0,
                    minified_size: None,
                    compression_ratio: None,
                    error: Some(format!("Failed to read file: {}", e)),
                }
            }
        };
        Self::minify_source(path, &source, config)
    }

    /// Minify source code
    pub fn minify_source(
        filename: &str,
        source: &str,
        config: Option<MinifyConfig>,
    ) -> MinifyResult {
        let config = config.unwrap_or_default();
        let original_size = source.len();
        let allocator = Allocator::default();
        let source_type = SourceType::from_path(filename)
            .unwrap_or_default()
            .with_module(true);

        // Parse the source
        let parser = Parser::new(&allocator, source, source_type);
        let mut ret = parser.parse();

        if !ret.errors.is_empty() {
            return MinifyResult {
                success: false,
                code: None,
                original_size,
                minified_size: None,
                compression_ratio: None,
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

        // Build minifier options
        let minifier_options = MinifierOptions {
            mangle: if config.mangle {
                Some(MangleOptions::default())
            } else {
                None
            },
            compress: if config.compress {
                Some(CompressOptions::default())
            } else {
                None
            },
        };

        // Run the minifier
        let minifier = OxcMinifier::new(minifier_options);
        minifier.minify(&allocator, &mut ret.program);

        // Generate output code with minified settings
        let codegen_options = CodegenOptions {
            minify: true,
            ..Default::default()
        };

        let code = Codegen::new()
            .with_options(codegen_options)
            .build(&ret.program)
            .code;

        let minified_size = code.len();
        let compression_ratio = if original_size > 0 {
            1.0 - (minified_size as f64 / original_size as f64)
        } else {
            0.0
        };

        MinifyResult {
            success: true,
            code: Some(code),
            original_size,
            minified_size: Some(minified_size),
            compression_ratio: Some(compression_ratio),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minify_simple() {
        let source = r#"
            function hello(name) {
                console.log("Hello, " + name);
            }
            hello("World");
        "#;

        let result = Minifier::minify_source("test.js", source, None);
        assert!(result.success);
        let code = result.code.unwrap();

        // Should be shorter
        assert!(code.len() < source.len());
        // Should remove whitespace
        assert!(!code.contains("    "));
    }

    #[test]
    fn test_minify_with_mangle() {
        let source = "function longVariableName(parameter) { return parameter * 2; }";

        let result = Minifier::minify_source("test.js", source, Some(MinifyConfig::default()));
        assert!(result.success);
        // Mangled code should be shorter
        assert!(result.minified_size.unwrap() < result.original_size);
    }

    #[test]
    fn test_minify_error() {
        let result = Minifier::minify_source("test.js", "function {}", None);
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_compression_ratio() {
        let source = r#"
            const veryLongVariableName = 1;
            const anotherLongVariable = 2;
            const result = veryLongVariableName + anotherLongVariable;
            console.log(result);
        "#;

        let result = Minifier::minify_source("test.js", source, None);
        assert!(result.success);
        assert!(result.compression_ratio.unwrap() > 0.0);
    }
}
