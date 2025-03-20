//! [`SyntaxChecker`] for AST-based syntax errors.
//!
//! This checker is not responsible for traversing the AST itself. Instead, its
//! [`SyntaxChecker::enter_stmt`] and [`SyntaxChecker::enter_expr`] methods should be called in a
//! parent `Visitor`'s `visit_stmt` and `visit_expr` methods, respectively.

use std::fmt::Display;

use ruff_python_ast::{
    self as ast,
    name::Name,
    visitor::{walk_expr, Visitor},
    Expr, PythonVersion, Stmt, StmtExpr, StmtImportFrom,
};
use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;

pub struct SemanticSyntaxChecker {
    /// The target Python version for detecting backwards-incompatible syntax
    /// changes.
    target_version: PythonVersion,
    /// The cumulative set of syntax errors found when visiting the source AST.
    errors: Vec<SemanticSyntaxError>,

    /// these could be grouped into a bitflags struct like `SemanticModel`
    seen_futures_boundary: bool,
    seen_docstring_boundary: bool,
}

impl SemanticSyntaxChecker {
    pub fn new(target_version: PythonVersion) -> Self {
        Self {
            target_version,
            errors: Vec::new(),
            seen_futures_boundary: false,
            seen_docstring_boundary: false,
        }
    }

    pub fn finish(self) -> Vec<SemanticSyntaxError> {
        self.errors
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SemanticSyntaxError {
    pub kind: SemanticSyntaxErrorKind,
    pub range: TextRange,
    pub target_version: PythonVersion,
}

impl Display for SemanticSyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            SemanticSyntaxErrorKind::LateFutureImport => {
                f.write_str("__future__ imports must be at the top of the file")
            }
            SemanticSyntaxErrorKind::ReboundComprehensionVariable => {
                f.write_str("assignment expression cannot rebind comprehension variable")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticSyntaxErrorKind {
    /// Represents the use of a `__future__` import after the beginning of a file.
    ///
    /// ## Examples
    ///
    /// ```python
    /// from pathlib import Path
    ///
    /// from __future__ import annotations
    /// ```
    ///
    /// This corresponds to the [`late-future-import`] (`F404`) rule in ruff.
    ///
    /// [`late-future-import`]: https://docs.astral.sh/ruff/rules/late-future-import/
    LateFutureImport,

    /// Represents the rebinding of the iteration variable of a list, set, or dict comprehension or
    /// a generator expression.
    ///
    /// ## Examples
    ///
    /// ```python
    /// [(a := 0) for a in range(0)]
    /// {(a := 0) for a in range(0)}
    /// {(a := 0): val for a in range(0)}
    /// {key: (a := 0) for a in range(0)}
    /// ((a := 0) for a in range(0))
    /// ```
    ReboundComprehensionVariable,
}

impl SemanticSyntaxChecker {
    fn check_stmt(&mut self, stmt: &ast::Stmt) {
        if let Stmt::ImportFrom(StmtImportFrom { range, module, .. }) = stmt {
            if self.seen_futures_boundary && matches!(module.as_deref(), Some("__future__")) {
                self.errors.push(SemanticSyntaxError {
                    kind: SemanticSyntaxErrorKind::LateFutureImport,
                    range: *range,
                    target_version: self.target_version,
                });
            }
        }
    }

    pub fn enter_stmt(&mut self, stmt: &ast::Stmt) {
        // update internal state
        match stmt {
            Stmt::Expr(StmtExpr { value, .. })
                if !self.seen_docstring_boundary && value.is_string_literal_expr() =>
            {
                self.seen_docstring_boundary = true;
            }
            Stmt::ImportFrom(StmtImportFrom { module, .. }) => {
                self.seen_docstring_boundary = true;
                // Allow __future__ imports until we see a non-__future__ import.
                if !matches!(module.as_deref(), Some("__future__")) {
                    self.seen_futures_boundary = true;
                }
            }
            _ => {
                self.seen_docstring_boundary = true;
                self.seen_futures_boundary = true;
            }
        }

        // check for errors
        self.check_stmt(stmt);
    }

    pub fn enter_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::ListComp(ast::ExprListComp {
                elt, generators, ..
            })
            | Expr::SetComp(ast::ExprSetComp {
                elt, generators, ..
            })
            | Expr::Generator(ast::ExprGenerator {
                elt, generators, ..
            }) => self.check_generator_expr(elt, generators),
            Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                ..
            }) => {
                self.check_generator_expr(key, generators);
                self.check_generator_expr(value, generators);
            }
            _ => {}
        }
    }

    /// Add a [`SyntaxErrorKind::ReboundComprehensionVariable`] if `expr` rebinds an iteration
    /// variable in `generators`.
    fn check_generator_expr(&mut self, expr: &Expr, generators: &[ast::Comprehension]) {
        let rebound_variables = {
            let mut visitor = ReboundComprehensionVisitor {
                iteration_variables: generators
                    .iter()
                    .filter_map(|gen| gen.target.as_name_expr().map(|name| &name.id))
                    .collect(),
                rebound_variables: Vec::new(),
            };
            visitor.visit_expr(expr);
            visitor.rebound_variables
        };

        // TODO(brent) with multiple diagnostic ranges, we could mark both the named expr (current)
        // and the name expr being rebound
        for range in rebound_variables {
            self.errors.push(SemanticSyntaxError {
                kind: SemanticSyntaxErrorKind::ReboundComprehensionVariable,
                range,
                target_version: self.target_version,
            });
        }
    }
}

/// Searches for the first named expression (`x := y`) rebinding one of the `iteration_variables` in
/// a comprehension or generator expression.
struct ReboundComprehensionVisitor<'a> {
    iteration_variables: FxHashSet<&'a Name>,
    rebound_variables: Vec<TextRange>,
}

impl Visitor<'_> for ReboundComprehensionVisitor<'_> {
    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Named(ast::ExprNamed { target, .. }) = expr {
            if let Expr::Name(ast::ExprName { id, range, .. }) = &**target {
                if self.iteration_variables.contains(id) {
                    self.rebound_variables.push(*range);
                }
            };
        }
        walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use insta::assert_debug_snapshot;
    use ruff_python_ast::{visitor::Visitor, PySourceType, PythonVersion};
    use ruff_python_trivia::textwrap::dedent;
    use test_case::test_case;

    use crate::{SemanticSyntaxChecker, SemanticSyntaxError};

    struct TestVisitor {
        checker: SemanticSyntaxChecker,
    }

    impl Visitor<'_> for TestVisitor {
        fn visit_stmt(&mut self, stmt: &ruff_python_ast::Stmt) {
            self.checker.enter_stmt(stmt);
            ruff_python_ast::visitor::walk_stmt(self, stmt);
        }

        fn visit_expr(&mut self, expr: &ruff_python_ast::Expr) {
            self.checker.enter_expr(expr);
            ruff_python_ast::visitor::walk_expr(self, expr);
        }
    }

    /// Run [`check_syntax`] on a snippet of Python code.
    fn test_snippet(contents: &str, target_version: PythonVersion) -> Vec<SemanticSyntaxError> {
        let path = Path::new("<filename>");
        let source_type = PySourceType::from(path);
        let parsed = ruff_python_parser::parse_unchecked_source(&dedent(contents), source_type);
        let mut visitor = TestVisitor {
            checker: SemanticSyntaxChecker::new(target_version),
        };

        for stmt in parsed.suite() {
            visitor.visit_stmt(stmt);
        }

        visitor.checker.errors
    }

    #[test_case("[(a := 0) for a in range(0)]", "listcomp")]
    #[test_case("{(a := 0) for a in range(0)}", "setcomp")]
    #[test_case("{(a := 0): val for a in range(0)}", "dictcomp_key")]
    #[test_case("{key: (a := 0) for a in range(0)}", "dictcomp_val")]
    #[test_case("((a := 0) for a in range(0))", "generator")]
    #[test_case("[[(a := 0)] for a in range(0)]", "nested_listcomp_expr")]
    #[test_case("[(a := 0) for b in range (0) for a in range(0)]", "nested_listcomp")]
    #[test_case("[(a := 0) for a in range (0) for b in range(0)]", "nested_listcomp2")]
    #[test_case(
        "[((a := 0), (b := 1)) for a in range (0) for b in range(0)]",
        "double_listcomp"
    )]
    fn rebound_comprehension_variable(contents: &str, name: &str) {
        assert_debug_snapshot!(name, test_snippet(contents, PythonVersion::default()));
    }
}
