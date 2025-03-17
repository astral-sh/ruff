//! [`SyntaxChecker`] for AST-based syntax errors.
//!
//! This checker is not responsible for traversing the AST itself. Instead, its
//! [`SyntaxChecker::enter_stmt`] method should be called on every node by a parent `Visitor`.

use ruff_python_ast::{self as ast, Expr, PythonVersion, Stmt, StmtExpr, StmtImportFrom};
use ruff_text_size::TextRange;

pub struct SyntaxChecker {
    /// The target Python version for detecting backwards-incompatible syntax
    /// changes.
    target_version: PythonVersion,
    /// The cumulative set of syntax errors found when visiting the source AST.
    errors: Vec<SyntaxError>,

    /// these could be grouped into a bitflags struct like `SemanticModel`
    seen_futures_boundary: bool,
    seen_docstring_boundary: bool,
}

impl SyntaxChecker {
    pub fn new(target_version: PythonVersion) -> Self {
        Self {
            target_version,
            errors: Vec::new(),
            seen_futures_boundary: false,
            seen_docstring_boundary: false,
        }
    }

    pub fn finish(&self) -> impl Iterator<Item = &SyntaxError> {
        self.errors.iter()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SyntaxError {
    pub kind: SyntaxErrorKind,
    pub range: TextRange,
    pub target_version: PythonVersion,
}

impl SyntaxError {
    pub fn message(&self) -> String {
        match self.kind {
            SyntaxErrorKind::LateFutureImport => {
                "__future__ imports must be at the top of the file".to_string()
            }
            SyntaxErrorKind::ReboundComprehensionVariable => {
                "assignment expression cannot rebind comprehension variable".to_string()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxErrorKind {
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

impl SyntaxErrorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            SyntaxErrorKind::LateFutureImport => "late-future-import",
            SyntaxErrorKind::ReboundComprehensionVariable => "rebound-comprehension-variable",
        }
    }
}

impl SyntaxChecker {
    fn check_stmt(&mut self, stmt: &ast::Stmt) {
        if let Stmt::ImportFrom(StmtImportFrom { range, module, .. }) = stmt {
            if self.seen_futures_boundary && matches!(module.as_deref(), Some("__future__")) {
                self.errors.push(SyntaxError {
                    kind: SyntaxErrorKind::LateFutureImport,
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

    #[allow(unused)]
    pub fn enter_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::ListComp(ast::ExprListComp {
                range,
                elt,
                generators,
            }) => {
                let Expr::Named(ast::ExprNamed { target, range, .. }) = &**elt else {
                    return;
                };
                let Expr::Name(ast::ExprName { id, .. }) = &**target else {
                    return;
                };
                if generators
                    .iter()
                    .any(|gen| gen.target.as_name_expr().is_some_and(|name| name.id == *id))
                {
                    self.errors.push(SyntaxError {
                        kind: SyntaxErrorKind::ReboundComprehensionVariable,
                        range: *range,
                        target_version: self.target_version,
                    });
                }
            }
            Expr::SetComp(ast::ExprSetComp {
                range,
                elt,
                generators,
            }) => todo!("set comprehension"),
            Expr::DictComp(ast::ExprDictComp {
                range,
                key,
                value,
                generators,
            }) => todo!("dict comprehension"),
            Expr::Generator(ast::ExprGenerator {
                range,
                elt,
                generators,
                parenthesized,
            }) => todo!("generator"),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use insta::assert_debug_snapshot;
    use ruff_python_ast::{visitor::Visitor, PySourceType, PythonVersion};
    use ruff_python_trivia::textwrap::dedent;
    use test_case::test_case;

    use crate::{SyntaxChecker, SyntaxError};

    struct TestVisitor {
        checker: SyntaxChecker,
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
    fn test_snippet(contents: &str, target_version: PythonVersion) -> Vec<SyntaxError> {
        let path = Path::new("<filename>");
        let source_type = PySourceType::from(path);
        let parsed = ruff_python_parser::parse_unchecked_source(&dedent(contents), source_type);
        let mut visitor = TestVisitor {
            checker: SyntaxChecker::new(target_version),
        };

        for stmt in parsed.suite() {
            visitor.visit_stmt(stmt);
        }

        visitor.checker.errors
    }

    #[test_case("[(a := 0) for a in range(0)]", "listcomp")]
    fn expr(contents: &str, name: &str) {
        assert_debug_snapshot!(name, test_snippet(contents, PythonVersion::default()));
    }
}
