//! JavaScript/TypeScript parser powered by OXC
//!
//! Parse source code and return AST as JSON.

use oxc::allocator::Allocator;
use oxc::ast::ast::Program;
use oxc::parser::Parser;
use oxc::span::SourceType;
use serde::{Deserialize, Serialize};
use std::fs;

/// Parse result containing AST information
#[derive(Debug, Serialize, Deserialize)]
pub struct ParseResult {
    /// Whether parsing succeeded without errors
    pub success: bool,
    /// Parse errors if any
    pub errors: Vec<ParseError>,
    /// Program information (simplified AST summary)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<ProgramInfo>,
}

/// A parse error
#[derive(Debug, Serialize, Deserialize)]
pub struct ParseError {
    pub message: String,
    pub line: u32,
    pub column: u32,
}

/// Simplified program information
#[derive(Debug, Serialize, Deserialize)]
pub struct ProgramInfo {
    /// Source type (script or module)
    pub source_type: String,
    /// Whether the file uses strict mode
    pub has_use_strict: bool,
    /// Number of statements
    pub statement_count: usize,
    /// Imports found
    pub imports: Vec<ImportInfo>,
    /// Exports found
    pub exports: Vec<ExportInfo>,
    /// Functions declared
    pub functions: Vec<FunctionInfo>,
    /// Classes declared
    pub classes: Vec<ClassInfo>,
    /// Variables declared at top level
    pub variables: Vec<VariableInfo>,
}

/// Import information
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportInfo {
    pub source: String,
    pub specifiers: Vec<String>,
    pub is_type_only: bool,
}

/// Export information
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportInfo {
    pub name: String,
    pub is_default: bool,
    pub is_type_only: bool,
}

/// Function information
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: Option<String>,
    pub is_async: bool,
    pub is_generator: bool,
    pub param_count: usize,
}

/// Class information
#[derive(Debug, Serialize, Deserialize)]
pub struct ClassInfo {
    pub name: Option<String>,
    pub has_super: bool,
    pub method_count: usize,
}

/// Variable information
#[derive(Debug, Serialize, Deserialize)]
pub struct VariableInfo {
    pub name: String,
    pub kind: String,
}

/// AST Parser
pub struct AstParser;

impl AstParser {
    /// Parse a file and return structured information
    pub fn parse_file(path: &str) -> Result<ParseResult, String> {
        let source = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
        Self::parse_source(path, &source)
    }

    /// Parse source code and return structured information
    pub fn parse_source(filename: &str, source: &str) -> Result<ParseResult, String> {
        let allocator = Allocator::default();
        let source_type = SourceType::from_path(filename).unwrap_or_default();

        let parser = Parser::new(&allocator, source, source_type);
        let ret = parser.parse();

        if !ret.errors.is_empty() {
            return Ok(ParseResult {
                success: false,
                errors: ret
                    .errors
                    .iter()
                    .map(|e| {
                        // Extract offset from error labels if available
                        let offset = e
                            .labels
                            .as_ref()
                            .and_then(|labels| labels.first())
                            .map(|label| label.offset())
                            .unwrap_or(0);

                        let (line, column) = Self::offset_to_line_col(source, offset);
                        ParseError {
                            message: e.to_string(),
                            line,
                            column,
                        }
                    })
                    .collect(),
                program: None,
            });
        }

        let program_info = Self::extract_program_info(&ret.program);

        Ok(ParseResult {
            success: true,
            errors: vec![],
            program: Some(program_info),
        })
    }

    /// Convert byte offset to line and column numbers (1-indexed)
    fn offset_to_line_col(source: &str, offset: usize) -> (u32, u32) {
        let mut line = 1u32;
        let mut col = 1u32;

        for (i, ch) in source.char_indices() {
            if i >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }

        (line, col)
    }

    fn extract_program_info(program: &Program) -> ProgramInfo {
        use oxc::ast::ast::*;

        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let mut functions = Vec::new();
        let mut classes = Vec::new();
        let mut variables = Vec::new();
        let mut has_use_strict = false;

        for stmt in &program.body {
            match stmt {
                Statement::ExpressionStatement(expr_stmt) => {
                    if let Expression::StringLiteral(lit) = &expr_stmt.expression {
                        if lit.value == "use strict" {
                            has_use_strict = true;
                        }
                    }
                }
                Statement::ImportDeclaration(import) => {
                    let specifiers: Vec<String> = import
                        .specifiers
                        .as_ref()
                        .map(|specs| {
                            specs
                                .iter()
                                .map(|s| match s {
                                    ImportDeclarationSpecifier::ImportSpecifier(spec) => {
                                        spec.local.name.to_string()
                                    }
                                    ImportDeclarationSpecifier::ImportDefaultSpecifier(spec) => {
                                        spec.local.name.to_string()
                                    }
                                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(spec) => {
                                        format!("* as {}", spec.local.name)
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    imports.push(ImportInfo {
                        source: import.source.value.to_string(),
                        specifiers,
                        is_type_only: import.import_kind.is_type(),
                    });
                }
                Statement::ExportDefaultDeclaration(export) => {
                    let name = match &export.declaration {
                        ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                            f.id.as_ref()
                                .map(|id| id.name.to_string())
                                .unwrap_or_else(|| "default".to_string())
                        }
                        ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                            c.id.as_ref()
                                .map(|id| id.name.to_string())
                                .unwrap_or_else(|| "default".to_string())
                        }
                        _ => "default".to_string(),
                    };
                    exports.push(ExportInfo {
                        name,
                        is_default: true,
                        is_type_only: false,
                    });
                }
                Statement::ExportNamedDeclaration(export) => {
                    if let Some(decl) = &export.declaration {
                        match decl {
                            Declaration::VariableDeclaration(var) => {
                                for declarator in &var.declarations {
                                    if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                                        exports.push(ExportInfo {
                                            name: id.name.to_string(),
                                            is_default: false,
                                            is_type_only: export.export_kind.is_type(),
                                        });
                                    }
                                }
                            }
                            Declaration::FunctionDeclaration(func) => {
                                if let Some(id) = &func.id {
                                    exports.push(ExportInfo {
                                        name: id.name.to_string(),
                                        is_default: false,
                                        is_type_only: false,
                                    });
                                }
                            }
                            Declaration::ClassDeclaration(class) => {
                                if let Some(id) = &class.id {
                                    exports.push(ExportInfo {
                                        name: id.name.to_string(),
                                        is_default: false,
                                        is_type_only: false,
                                    });
                                }
                            }
                            Declaration::TSTypeAliasDeclaration(alias) => {
                                exports.push(ExportInfo {
                                    name: alias.id.name.to_string(),
                                    is_default: false,
                                    is_type_only: true,
                                });
                            }
                            Declaration::TSInterfaceDeclaration(iface) => {
                                exports.push(ExportInfo {
                                    name: iface.id.name.to_string(),
                                    is_default: false,
                                    is_type_only: true,
                                });
                            }
                            _ => {}
                        }
                    }
                }
                Statement::FunctionDeclaration(func) => {
                    functions.push(FunctionInfo {
                        name: func.id.as_ref().map(|id| id.name.to_string()),
                        is_async: func.r#async,
                        is_generator: func.generator,
                        param_count: func.params.items.len(),
                    });
                }
                Statement::ClassDeclaration(class) => {
                    let method_count = class
                        .body
                        .body
                        .iter()
                        .filter(|e| matches!(e, ClassElement::MethodDefinition(_)))
                        .count();

                    classes.push(ClassInfo {
                        name: class.id.as_ref().map(|id| id.name.to_string()),
                        has_super: class.super_class.is_some(),
                        method_count,
                    });
                }
                Statement::VariableDeclaration(var) => {
                    let kind = match var.kind {
                        VariableDeclarationKind::Var => "var",
                        VariableDeclarationKind::Let => "let",
                        VariableDeclarationKind::Const => "const",
                        VariableDeclarationKind::Using => "using",
                        VariableDeclarationKind::AwaitUsing => "await using",
                    };
                    for declarator in &var.declarations {
                        if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                            variables.push(VariableInfo {
                                name: id.name.to_string(),
                                kind: kind.to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        ProgramInfo {
            source_type: if program.source_type.is_module() {
                "module"
            } else {
                "script"
            }
            .to_string(),
            has_use_strict,
            statement_count: program.body.len(),
            imports,
            exports,
            functions,
            classes,
            variables,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let result = AstParser::parse_source("test.js", "const x = 1;").unwrap();
        assert!(result.success);
        assert!(result.program.is_some());
        assert_eq!(result.program.as_ref().unwrap().variables.len(), 1);
    }

    #[test]
    fn test_parse_imports() {
        let result =
            AstParser::parse_source("test.ts", "import { foo } from 'bar'; export const x = 1;")
                .unwrap();
        assert!(result.success);
        let program = result.program.unwrap();
        assert_eq!(program.imports.len(), 1);
        assert_eq!(program.imports[0].source, "bar");
        assert_eq!(program.exports.len(), 1);
    }

    #[test]
    fn test_parse_error() {
        let result = AstParser::parse_source("test.js", "const x = ").unwrap();
        assert!(!result.success);
        assert!(!result.errors.is_empty());
    }
}
