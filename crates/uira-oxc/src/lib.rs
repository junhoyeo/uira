//! # uira-oxc
//!
//! OXC-powered JavaScript/TypeScript tools for Uira.
//!
//! Provides native Rust implementations for:
//! - **Linting** - Fast JS/TS linting with customizable rules
//! - **Parsing** - Parse to AST and return as JSON
//! - **Transforming** - Transpile TypeScript/JSX to JavaScript
//! - **Minifying** - Minify JavaScript code

pub mod linter;
pub mod minifier;
pub mod parser;
pub mod transformer;

pub use linter::{LintDiagnostic, LintRule, Linter, Severity};
pub use minifier::Minifier;
pub use parser::AstParser;
pub use transformer::Transformer;
