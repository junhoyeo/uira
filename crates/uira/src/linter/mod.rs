use anyhow::Result;
use colored::Colorize;
use oxc::allocator::Allocator;
use oxc::ast::ast::{CallExpression, Expression};
use oxc::ast_visit::{walk, Visit};
use oxc::parser::Parser;
use oxc::span::SourceType;
use rayon::prelude::*;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub rule: String,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

pub struct Linter {
    pub no_console: bool,
    pub no_debugger: bool,
}

impl Default for Linter {
    fn default() -> Self {
        Self {
            no_console: true,
            no_debugger: true,
        }
    }
}

impl Linter {
    pub fn lint_files(&self, files: &[String]) -> Result<Vec<LintDiagnostic>> {
        let lintable_files: Vec<_> = files.iter().filter(|f| is_lintable(f)).collect();

        if lintable_files.is_empty() {
            return Ok(vec![]);
        }

        let diagnostics: Vec<LintDiagnostic> = lintable_files
            .par_iter()
            .flat_map(|file| self.lint_file(file).unwrap_or_default())
            .collect();

        Ok(diagnostics)
    }

    pub fn lint_file(&self, path: &str) -> Result<Vec<LintDiagnostic>> {
        let source = fs::read_to_string(path)?;
        let allocator = Allocator::default();
        let source_type = SourceType::from_path(path).unwrap_or_default();

        let parser = Parser::new(&allocator, &source, source_type);
        let ret = parser.parse();

        if !ret.errors.is_empty() {
            return Ok(vec![]);
        }

        let mut visitor = LintVisitor::new(path.to_string(), &source, self);
        visitor.visit_program(&ret.program);

        Ok(visitor.diagnostics)
    }

    pub fn run(&self, files: &[String]) -> Result<bool> {
        let error_count = AtomicUsize::new(0);
        let warning_count = AtomicUsize::new(0);

        let diagnostics = self.lint_files(files)?;

        for d in &diagnostics {
            match d.severity {
                Severity::Error => error_count.fetch_add(1, Ordering::Relaxed),
                Severity::Warning => warning_count.fetch_add(1, Ordering::Relaxed),
            };

            let severity_str = match d.severity {
                Severity::Error => "error".red().bold(),
                Severity::Warning => "warning".yellow().bold(),
            };

            println!(
                "{}:{}:{}: {} [{}]",
                d.file.dimmed(),
                d.line,
                d.column,
                severity_str,
                d.rule.cyan()
            );
            println!("  {}", d.message);
            println!();
        }

        let errors = error_count.load(Ordering::Relaxed);
        let warnings = warning_count.load(Ordering::Relaxed);

        if errors > 0 || warnings > 0 {
            println!(
                "{} {} error(s), {} warning(s)",
                "✗".red().bold(),
                errors,
                warnings
            );
        } else if !files.is_empty() {
            println!("{} No issues found", "✓".green().bold());
        }

        Ok(errors == 0)
    }
}

struct LintVisitor<'a> {
    file: String,
    source: &'a str,
    config: &'a Linter,
    diagnostics: Vec<LintDiagnostic>,
}

impl<'a> LintVisitor<'a> {
    fn new(file: String, source: &'a str, config: &'a Linter) -> Self {
        Self {
            file,
            source,
            config,
            diagnostics: Vec::new(),
        }
    }

    fn get_line_col(&self, offset: u32) -> (u32, u32) {
        let mut line = 1u32;
        let mut col = 1u32;

        for (i, ch) in self.source.char_indices() {
            if i as u32 >= offset {
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

    fn add_diagnostic(&mut self, offset: u32, message: &str, rule: &str, severity: Severity) {
        let (line, column) = self.get_line_col(offset);
        self.diagnostics.push(LintDiagnostic {
            file: self.file.clone(),
            line,
            column,
            message: message.to_string(),
            rule: rule.to_string(),
            severity,
        });
    }
}

impl<'a> Visit<'a> for LintVisitor<'a> {
    fn visit_debugger_statement(&mut self, stmt: &oxc::ast::ast::DebuggerStatement) {
        if self.config.no_debugger {
            self.add_diagnostic(
                stmt.span.start,
                "Unexpected 'debugger' statement",
                "no-debugger",
                Severity::Error,
            );
        }
    }

    fn visit_call_expression(&mut self, expr: &CallExpression<'a>) {
        if self.config.no_console {
            if let Expression::StaticMemberExpression(member) = &expr.callee {
                if let Expression::Identifier(id) = &member.object {
                    if id.name == "console" {
                        self.add_diagnostic(
                            expr.span.start,
                            "Unexpected console statement",
                            "no-console",
                            Severity::Warning,
                        );
                    }
                }
            }
        }

        walk::walk_call_expression(self, expr);
    }
}

fn is_lintable(path: &str) -> bool {
    let path = Path::new(path);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    matches!(
        ext,
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_console() {
        let linter = Linter::default();
        let source = r#"console.log("hello");"#;

        let allocator = Allocator::default();
        let source_type = SourceType::from_path("test.js").unwrap();
        let parser = Parser::new(&allocator, source, source_type);
        let ret = parser.parse();

        let mut visitor = LintVisitor::new("test.js".to_string(), source, &linter);
        visitor.visit_program(&ret.program);

        assert_eq!(visitor.diagnostics.len(), 1);
        assert_eq!(visitor.diagnostics[0].rule, "no-console");
    }

    #[test]
    fn test_no_debugger() {
        let linter = Linter::default();
        let source = r#"debugger;"#;

        let allocator = Allocator::default();
        let source_type = SourceType::from_path("test.js").unwrap();
        let parser = Parser::new(&allocator, source, source_type);
        let ret = parser.parse();

        let mut visitor = LintVisitor::new("test.js".to_string(), source, &linter);
        visitor.visit_program(&ret.program);

        assert_eq!(visitor.diagnostics.len(), 1);
        assert_eq!(visitor.diagnostics[0].rule, "no-debugger");
    }

    #[test]
    fn test_clean_code() {
        let linter = Linter::default();
        let source = r#"const x = 1;"#;

        let allocator = Allocator::default();
        let source_type = SourceType::from_path("test.js").unwrap();
        let parser = Parser::new(&allocator, source, source_type);
        let ret = parser.parse();

        let mut visitor = LintVisitor::new("test.js".to_string(), source, &linter);
        visitor.visit_program(&ret.program);

        assert!(visitor.diagnostics.is_empty());
    }
}
