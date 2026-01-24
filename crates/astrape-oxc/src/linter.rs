//! JavaScript/TypeScript linting powered by OXC
//!
//! Fast linting with customizable rules.

use oxc::allocator::Allocator;
use oxc::ast::ast::{
    AssignmentTarget, BindingPattern, CallExpression, Expression, VariableDeclarationKind,
};
use oxc::ast_visit::{walk, Visit};
use oxc::parser::Parser;
use oxc::semantic::ScopeFlags;
use oxc::span::SourceType;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Lint diagnostic representing a single issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintDiagnostic {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub rule: String,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// Severity level for diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Available lint rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LintRule {
    /// Disallow console.* calls
    NoConsole,
    /// Disallow debugger statements
    NoDebugger,
    /// Disallow alert/confirm/prompt
    NoAlert,
    /// Disallow eval()
    NoEval,
    /// Disallow var declarations (prefer let/const)
    NoVar,
    /// Prefer const over let when variable is never reassigned
    PreferConst,
    /// Disallow unused variables
    NoUnusedVars,
    /// Disallow empty functions
    NoEmptyFunction,
    /// Disallow duplicate keys in objects
    NoDuplicateKeys,
    /// Disallow reassigning function parameters
    NoParamReassign,
}

impl LintRule {
    /// Get all available rules
    pub fn all() -> Vec<LintRule> {
        vec![
            LintRule::NoConsole,
            LintRule::NoDebugger,
            LintRule::NoAlert,
            LintRule::NoEval,
            LintRule::NoVar,
            LintRule::PreferConst,
            LintRule::NoUnusedVars,
            LintRule::NoEmptyFunction,
            LintRule::NoDuplicateKeys,
            LintRule::NoParamReassign,
        ]
    }

    /// Get recommended rules (good defaults)
    pub fn recommended() -> Vec<LintRule> {
        vec![
            LintRule::NoDebugger,
            LintRule::NoEval,
            LintRule::NoVar,
            LintRule::NoDuplicateKeys,
        ]
    }

    /// Get strict rules (all enabled)
    pub fn strict() -> Vec<LintRule> {
        Self::all()
    }

    /// Get rule description
    pub fn description(&self) -> &'static str {
        match self {
            LintRule::NoConsole => "Disallow console.* calls in production code",
            LintRule::NoDebugger => "Disallow debugger statements",
            LintRule::NoAlert => "Disallow alert(), confirm(), and prompt()",
            LintRule::NoEval => "Disallow eval() which can be a security risk",
            LintRule::NoVar => "Prefer let/const over var for block scoping",
            LintRule::PreferConst => "Use const for variables that are never reassigned",
            LintRule::NoUnusedVars => "Disallow unused variables",
            LintRule::NoEmptyFunction => "Disallow empty function bodies",
            LintRule::NoDuplicateKeys => "Disallow duplicate keys in object literals",
            LintRule::NoParamReassign => "Disallow reassigning function parameters",
        }
    }
}

/// The linter configuration and executor
pub struct Linter {
    rules: HashSet<LintRule>,
}

impl Default for Linter {
    fn default() -> Self {
        Self {
            rules: LintRule::recommended().into_iter().collect(),
        }
    }
}

impl Linter {
    /// Create a new linter with specified rules
    pub fn new(rules: Vec<LintRule>) -> Self {
        Self {
            rules: rules.into_iter().collect(),
        }
    }

    /// Create a linter with all rules enabled
    pub fn strict() -> Self {
        Self::new(LintRule::strict())
    }

    /// Check if a rule is enabled
    pub fn has_rule(&self, rule: LintRule) -> bool {
        self.rules.contains(&rule)
    }

    /// Lint multiple files in parallel
    pub fn lint_files(&self, files: &[String]) -> Vec<LintDiagnostic> {
        let lintable_files: Vec<_> = files.iter().filter(|f| is_lintable(f)).collect();

        if lintable_files.is_empty() {
            return vec![];
        }

        lintable_files
            .par_iter()
            .flat_map(|file| self.lint_file(file).unwrap_or_default())
            .collect()
    }

    /// Lint a single file
    pub fn lint_file(&self, path: &str) -> Result<Vec<LintDiagnostic>, String> {
        let source = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
        self.lint_source(path, &source)
    }

    /// Lint source code directly
    pub fn lint_source(&self, filename: &str, source: &str) -> Result<Vec<LintDiagnostic>, String> {
        let allocator = Allocator::default();
        let source_type = SourceType::from_path(filename).unwrap_or_default();

        let parser = Parser::new(&allocator, source, source_type);
        let ret = parser.parse();

        // Return parse errors as diagnostics
        if !ret.errors.is_empty() {
            return Ok(ret
                .errors
                .iter()
                .map(|e| LintDiagnostic {
                    file: filename.to_string(),
                    line: 1,
                    column: 1,
                    message: e.to_string(),
                    rule: "parse-error".to_string(),
                    severity: Severity::Error,
                    suggestion: None,
                })
                .collect());
        }

        let mut visitor = LintVisitor::new(filename.to_string(), source, self);
        visitor.visit_program(&ret.program);

        // Finalize to emit diagnostics for rules that require post-processing
        visitor.finalize();

        Ok(visitor.diagnostics)
    }
}

struct LintVisitor<'a> {
    file: String,
    source: &'a str,
    config: &'a Linter,
    diagnostics: Vec<LintDiagnostic>,
    // Track declared variables for no-unused-vars
    declared_vars: HashSet<String>,
    used_vars: HashSet<String>,
    // Track let declarations for prefer-const
    let_declarations: Vec<(String, u32, bool)>, // (name, offset, reassigned)
    // Track object keys for no-duplicate-keys
    current_object_keys: Vec<HashSet<String>>,
    // Track function parameters for no-param-reassign
    function_params: Vec<HashSet<String>>,
}

impl<'a> LintVisitor<'a> {
    fn new(file: String, source: &'a str, config: &'a Linter) -> Self {
        Self {
            file,
            source,
            config,
            diagnostics: Vec::new(),
            declared_vars: HashSet::new(),
            used_vars: HashSet::new(),
            let_declarations: Vec::new(),
            current_object_keys: Vec::new(),
            function_params: Vec::new(),
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

    fn add_diagnostic(
        &mut self,
        offset: u32,
        message: &str,
        rule: &str,
        severity: Severity,
        suggestion: Option<&str>,
    ) {
        let (line, column) = self.get_line_col(offset);
        self.diagnostics.push(LintDiagnostic {
            file: self.file.clone(),
            line,
            column,
            message: message.to_string(),
            rule: rule.to_string(),
            severity,
            suggestion: suggestion.map(|s| s.to_string()),
        });
    }

    /// Finalize linting and emit diagnostics for rules that require post-processing
    fn finalize(&mut self) {
        // PreferConst: emit warnings for let declarations that were never reassigned
        if self.config.has_rule(LintRule::PreferConst) {
            // Collect diagnostics first to avoid borrow conflicts
            let const_diagnostics: Vec<_> = self
                .let_declarations
                .iter()
                .filter(|(_, _, reassigned)| !reassigned)
                .map(|(name, offset, _)| {
                    let (line, column) = self.get_line_col(*offset);
                    LintDiagnostic {
                        file: self.file.clone(),
                        line,
                        column,
                        message: format!("'{}' is never reassigned. Use 'const' instead", name),
                        rule: "prefer-const".to_string(),
                        severity: Severity::Warning,
                        suggestion: Some("Replace 'let' with 'const'".to_string()),
                    }
                })
                .collect();
            self.diagnostics.extend(const_diagnostics);
        }

        // NoUnusedVars: emit warnings for declared variables that were never used
        if self.config.has_rule(LintRule::NoUnusedVars) {
            let unused_diagnostics: Vec<_> = self
                .declared_vars
                .difference(&self.used_vars)
                .filter(|name| !name.starts_with('_')) // Skip intentionally unused
                .map(|name| LintDiagnostic {
                    file: self.file.clone(),
                    line: 1, // We don't track declaration location for this rule
                    column: 1,
                    message: format!("'{}' is declared but never used", name),
                    rule: "no-unused-vars".to_string(),
                    severity: Severity::Warning,
                    suggestion: Some(
                        "Remove the unused variable or prefix with underscore".to_string(),
                    ),
                })
                .collect();
            self.diagnostics.extend(unused_diagnostics);
        }
    }
}

impl<'a> Visit<'a> for LintVisitor<'a> {
    fn visit_debugger_statement(&mut self, stmt: &oxc::ast::ast::DebuggerStatement) {
        if self.config.has_rule(LintRule::NoDebugger) {
            self.add_diagnostic(
                stmt.span.start,
                "Unexpected 'debugger' statement",
                "no-debugger",
                Severity::Error,
                Some("Remove the debugger statement before committing"),
            );
        }
    }

    fn visit_call_expression(&mut self, expr: &CallExpression<'a>) {
        // no-console
        if self.config.has_rule(LintRule::NoConsole) {
            if let Expression::StaticMemberExpression(member) = &expr.callee {
                if let Expression::Identifier(id) = &member.object {
                    if id.name == "console" {
                        self.add_diagnostic(
                            expr.span.start,
                            &format!("Unexpected console.{} call", member.property.name),
                            "no-console",
                            Severity::Warning,
                            Some("Remove console calls or use a proper logging library"),
                        );
                    }
                }
            }
        }

        // no-alert
        if self.config.has_rule(LintRule::NoAlert) {
            if let Expression::Identifier(id) = &expr.callee {
                if matches!(id.name.as_str(), "alert" | "confirm" | "prompt") {
                    self.add_diagnostic(
                        expr.span.start,
                        &format!("Unexpected {}() call", id.name),
                        "no-alert",
                        Severity::Warning,
                        Some("Use a modal or toast library instead"),
                    );
                }
            }
        }

        // no-eval
        if self.config.has_rule(LintRule::NoEval) {
            if let Expression::Identifier(id) = &expr.callee {
                if id.name == "eval" {
                    self.add_diagnostic(
                        expr.span.start,
                        "eval() is a security risk and should be avoided",
                        "no-eval",
                        Severity::Error,
                        Some("Use safer alternatives like JSON.parse() for data or restructure code to avoid dynamic evaluation"),
                    );
                }
            }
        }

        // Track used variables
        if let Expression::Identifier(id) = &expr.callee {
            self.used_vars.insert(id.name.to_string());
        }

        walk::walk_call_expression(self, expr);
    }

    fn visit_variable_declaration(&mut self, decl: &oxc::ast::ast::VariableDeclaration<'a>) {
        // no-var
        if self.config.has_rule(LintRule::NoVar) && decl.kind == VariableDeclarationKind::Var {
            self.add_diagnostic(
                decl.span.start,
                "Unexpected var, use let or const instead",
                "no-var",
                Severity::Warning,
                Some("Replace 'var' with 'let' or 'const'"),
            );
        }

        // Track declarations for prefer-const
        if self.config.has_rule(LintRule::PreferConst) && decl.kind == VariableDeclarationKind::Let
        {
            for declarator in &decl.declarations {
                if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                    self.let_declarations
                        .push((id.name.to_string(), decl.span.start, false));
                }
            }
        }

        // Track declared variables for no-unused-vars
        if self.config.has_rule(LintRule::NoUnusedVars) {
            for declarator in &decl.declarations {
                if let BindingPattern::BindingIdentifier(id) = &declarator.id {
                    self.declared_vars.insert(id.name.to_string());
                }
            }
        }

        walk::walk_variable_declaration(self, decl);
    }

    fn visit_assignment_expression(&mut self, expr: &oxc::ast::ast::AssignmentExpression<'a>) {
        // Track reassignments for prefer-const
        if let AssignmentTarget::AssignmentTargetIdentifier(id) = &expr.left {
            let name = id.name.to_string();
            for (var_name, _, reassigned) in &mut self.let_declarations {
                if *var_name == name {
                    *reassigned = true;
                }
            }

            // no-param-reassign
            if self.config.has_rule(LintRule::NoParamReassign) {
                if let Some(params) = self.function_params.last() {
                    if params.contains(&name) {
                        self.add_diagnostic(
                            expr.span.start,
                            &format!("Assignment to function parameter '{}'", name),
                            "no-param-reassign",
                            Severity::Warning,
                            Some("Create a new variable instead of reassigning the parameter"),
                        );
                    }
                }
            }
        }

        walk::walk_assignment_expression(self, expr);
    }

    fn visit_function(&mut self, func: &oxc::ast::ast::Function<'a>, _flags: ScopeFlags) {
        // Track function parameters
        if self.config.has_rule(LintRule::NoParamReassign) {
            let mut params = HashSet::new();
            for param in &func.params.items {
                if let BindingPattern::BindingIdentifier(id) = &param.pattern {
                    params.insert(id.name.to_string());
                }
            }
            self.function_params.push(params);
        }

        // no-empty-function
        if self.config.has_rule(LintRule::NoEmptyFunction) {
            if let Some(body) = &func.body {
                if body.statements.is_empty() {
                    self.add_diagnostic(
                        func.span.start,
                        "Unexpected empty function",
                        "no-empty-function",
                        Severity::Warning,
                        Some("Add a comment or implementation, or remove if unused"),
                    );
                }
            }
        }

        walk::walk_function(self, func, _flags);

        // Pop function params after visiting
        if self.config.has_rule(LintRule::NoParamReassign) {
            self.function_params.pop();
        }
    }

    fn visit_object_expression(&mut self, obj: &oxc::ast::ast::ObjectExpression<'a>) {
        // Track object keys for no-duplicate-keys
        if self.config.has_rule(LintRule::NoDuplicateKeys) {
            let mut keys = HashSet::new();
            for prop in &obj.properties {
                if let oxc::ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop {
                    if let oxc::ast::ast::PropertyKey::StaticIdentifier(id) = &p.key {
                        let key_name = id.name.to_string();
                        if keys.contains(&key_name) {
                            self.add_diagnostic(
                                p.span.start,
                                &format!("Duplicate key '{}'", key_name),
                                "no-duplicate-keys",
                                Severity::Error,
                                Some("Remove the duplicate key or rename one of them"),
                            );
                        } else {
                            keys.insert(key_name);
                        }
                    }
                }
            }
            self.current_object_keys.push(keys);
        }

        walk::walk_object_expression(self, obj);

        if self.config.has_rule(LintRule::NoDuplicateKeys) {
            self.current_object_keys.pop();
        }
    }

    fn visit_identifier_reference(&mut self, id: &oxc::ast::ast::IdentifierReference<'a>) {
        // Track used variables
        self.used_vars.insert(id.name.to_string());
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
        let linter = Linter::new(vec![LintRule::NoConsole]);
        let diagnostics = linter
            .lint_source("test.js", r#"console.log("hello");"#)
            .unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "no-console");
    }

    #[test]
    fn test_no_debugger() {
        let linter = Linter::new(vec![LintRule::NoDebugger]);
        let diagnostics = linter.lint_source("test.js", "debugger;").unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "no-debugger");
    }

    #[test]
    fn test_no_var() {
        let linter = Linter::new(vec![LintRule::NoVar]);
        let diagnostics = linter.lint_source("test.js", "var x = 1;").unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "no-var");
    }

    #[test]
    fn test_no_eval() {
        let linter = Linter::new(vec![LintRule::NoEval]);
        let diagnostics = linter.lint_source("test.js", r#"eval("x")"#).unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "no-eval");
    }

    #[test]
    fn test_no_duplicate_keys() {
        let linter = Linter::new(vec![LintRule::NoDuplicateKeys]);
        let diagnostics = linter
            .lint_source("test.js", "const obj = { a: 1, a: 2 };")
            .unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "no-duplicate-keys");
    }

    #[test]
    fn test_clean_code() {
        let linter = Linter::strict();
        let diagnostics = linter
            .lint_source("test.js", "const x = 1; function foo() { return x; }")
            .unwrap();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_prefer_const() {
        let linter = Linter::new(vec![LintRule::PreferConst]);
        let diagnostics = linter
            .lint_source("test.js", "let x = 1; console.log(x);")
            .unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "prefer-const");
        assert!(diagnostics[0].message.contains("never reassigned"));
    }

    #[test]
    fn test_prefer_const_reassigned() {
        let linter = Linter::new(vec![LintRule::PreferConst]);
        let diagnostics = linter.lint_source("test.js", "let x = 1; x = 2;").unwrap();

        // Should not warn because x is reassigned
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_no_unused_vars() {
        let linter = Linter::new(vec![LintRule::NoUnusedVars]);
        let diagnostics = linter.lint_source("test.js", "const unused = 1;").unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "no-unused-vars");
        assert!(diagnostics[0].message.contains("never used"));
    }

    #[test]
    fn test_no_unused_vars_underscore() {
        let linter = Linter::new(vec![LintRule::NoUnusedVars]);
        let diagnostics = linter
            .lint_source("test.js", "const _intentionallyUnused = 1;")
            .unwrap();

        // Should not warn for underscore-prefixed variables
        assert!(diagnostics.is_empty());
    }
}
