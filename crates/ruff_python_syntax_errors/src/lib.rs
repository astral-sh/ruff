//! [`SyntaxChecker`] for AST-based syntax errors.
//!
//! This checker is not responsible for traversing the AST itself. Instead, its
//! [`SyntaxChecker::enter_stmt`] and [`SyntaxChecker::enter_expr`] methods should be called in a
//! parent `Visitor`'s `visit_stmt` and `visit_expr` methods, respectively.

use std::{cell::RefCell, fmt::Display};

use ruff_python_ast::{
    self as ast,
    visitor::{walk_expr, Visitor},
    Expr, PythonVersion, Stmt, StmtExpr, StmtImportFrom,
};
use ruff_text_size::TextRange;

struct CheckerState {
    /// these could be grouped into a bitflags struct like `SemanticModel`
    seen_futures_boundary: bool,
}

pub struct SemanticSyntaxChecker {
    /// The cumulative set of syntax errors found when visiting the source AST.
    errors: RefCell<Vec<SemanticSyntaxError>>,

    state: RefCell<CheckerState>,
}

impl SemanticSyntaxChecker {
    pub fn new() -> Self {
        Self {
            errors: RefCell::new(Vec::new()),
            state: RefCell::new(CheckerState {
                seen_futures_boundary: false,
            }),
        }
    }

    pub fn finish(self) -> Vec<SemanticSyntaxError> {
        self.errors.into_inner()
    }

    fn seen_futures_boundary(&self) -> bool {
        self.state.borrow().seen_futures_boundary
    }

    fn set_seen_futures_boundary(&self, seen_futures_boundary: bool) {
        self.state.borrow_mut().seen_futures_boundary = seen_futures_boundary;
    }

    fn add_error(
        &self,
        kind: SemanticSyntaxErrorKind,
        range: TextRange,
        python_version: PythonVersion,
    ) {
        self.errors.borrow_mut().push(SemanticSyntaxError {
            kind,
            range,
            python_version,
        });
    }
}

impl Default for SemanticSyntaxChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SemanticSyntaxError {
    pub kind: SemanticSyntaxErrorKind,
    pub range: TextRange,
    pub python_version: PythonVersion,
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

pub trait SemanticSyntaxContext {
    /// Returns `true` if a module's docstring boundary has been passed.
    fn seen_docstring_boundary(&self) -> bool;

    /// The target Python version for detecting backwards-incompatible syntax changes.
    fn python_version(&self) -> PythonVersion;
}

impl SemanticSyntaxChecker {
    fn check_stmt<Ctx: SemanticSyntaxContext>(&self, stmt: &ast::Stmt, ctx: &Ctx) {
        if let Stmt::ImportFrom(StmtImportFrom { range, module, .. }) = stmt {
            if self.seen_futures_boundary() && matches!(module.as_deref(), Some("__future__")) {
                self.add_error(
                    SemanticSyntaxErrorKind::LateFutureImport,
                    *range,
                    ctx.python_version(),
                );
            }
        }
    }

    pub fn visit_stmt<Ctx: SemanticSyntaxContext>(&self, stmt: &ast::Stmt, ctx: &Ctx) {
        // update internal state
        match stmt {
            Stmt::Expr(StmtExpr { value, .. })
                if !ctx.seen_docstring_boundary() && value.is_string_literal_expr() => {}
            Stmt::ImportFrom(StmtImportFrom { module, .. }) => {
                // Allow __future__ imports until we see a non-__future__ import.
                if !matches!(module.as_deref(), Some("__future__")) {
                    self.set_seen_futures_boundary(true);
                }
            }
            _ => {
                self.set_seen_futures_boundary(true);
            }
        }

        // check for errors
        self.check_stmt(stmt, ctx);
    }

    pub fn visit_expr<Ctx: SemanticSyntaxContext>(&self, expr: &Expr, ctx: &Ctx) {
        match expr {
            Expr::ListComp(ast::ExprListComp {
                elt, generators, ..
            })
            | Expr::SetComp(ast::ExprSetComp {
                elt, generators, ..
            })
            | Expr::Generator(ast::ExprGenerator {
                elt, generators, ..
            }) => self.check_generator_expr(elt, generators, ctx),
            Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                ..
            }) => {
                self.check_generator_expr(key, generators, ctx);
                self.check_generator_expr(value, generators, ctx);
            }
            _ => {}
        }
    }

    /// Add a [`SyntaxErrorKind::ReboundComprehensionVariable`] if `expr` rebinds an iteration
    /// variable in `generators`.
    fn check_generator_expr<Ctx: SemanticSyntaxContext>(
        &self,
        expr: &Expr,
        comprehensions: &[ast::Comprehension],
        ctx: &Ctx,
    ) {
        let rebound_variables = {
            let mut visitor = ReboundComprehensionVisitor {
                comprehensions,
                rebound_variables: Vec::new(),
            };
            visitor.visit_expr(expr);
            visitor.rebound_variables
        };

        // TODO(brent) with multiple diagnostic ranges, we could mark both the named expr (current)
        // and the name expr being rebound
        for range in rebound_variables {
            self.add_error(
                SemanticSyntaxErrorKind::ReboundComprehensionVariable,
                range,
                ctx.python_version(),
            );
        }
    }
}

/// Searches for the first named expression (`x := y`) rebinding one of the `iteration_variables` in
/// a comprehension or generator expression.
struct ReboundComprehensionVisitor<'a> {
    comprehensions: &'a [ast::Comprehension],
    rebound_variables: Vec<TextRange>,
}

impl Visitor<'_> for ReboundComprehensionVisitor<'_> {
    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Named(ast::ExprNamed { target, .. }) = expr {
            if let Expr::Name(ast::ExprName { id, range, .. }) = &**target {
                if self.comprehensions.iter().any(|comp| {
                    comp.target
                        .as_name_expr()
                        .is_some_and(|name| name.id == *id)
                }) {
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

    use crate::{SemanticSyntaxChecker, SemanticSyntaxContext, SemanticSyntaxError};

    struct TestVisitor {
        checker: SemanticSyntaxChecker,
    }

    impl SemanticSyntaxContext for TestVisitor {
        fn seen_docstring_boundary(&self) -> bool {
            false
        }

        fn python_version(&self) -> PythonVersion {
            PythonVersion::default()
        }
    }

    impl Visitor<'_> for TestVisitor {
        fn visit_stmt(&mut self, stmt: &ruff_python_ast::Stmt) {
            self.checker.visit_stmt(stmt, self);
            ruff_python_ast::visitor::walk_stmt(self, stmt);
        }

        fn visit_expr(&mut self, expr: &ruff_python_ast::Expr) {
            self.checker.visit_expr(expr, self);
            ruff_python_ast::visitor::walk_expr(self, expr);
        }
    }

    /// Run [`check_syntax`] on a snippet of Python code.
    fn test_snippet(contents: &str) -> Vec<SemanticSyntaxError> {
        let path = Path::new("<filename>");
        let source_type = PySourceType::from(path);
        let parsed = ruff_python_parser::parse_unchecked_source(&dedent(contents), source_type);
        let mut visitor = TestVisitor {
            checker: SemanticSyntaxChecker::new(),
        };

        for stmt in parsed.suite() {
            visitor.visit_stmt(stmt);
        }

        visitor.checker.finish()
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
        assert_debug_snapshot!(name, test_snippet(contents));
    }
}
