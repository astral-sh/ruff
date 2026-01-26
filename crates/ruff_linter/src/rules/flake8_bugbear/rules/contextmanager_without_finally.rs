use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{self as ast, Expr, Stmt, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `@contextlib.contextmanager` decorated functions that contain
/// `yield` expressions not wrapped in a `try`/`finally` block.
///
/// ## Why is this bad?
/// When a context manager is used, code after `yield` is intended to run
/// during cleanup when the context exits. However, if an exception is raised
/// inside the `with` block, code after an unprotected `yield` will never
/// execute.
///
/// Wrapping `yield` in a `try`/`finally` block ensures that cleanup code in
/// the `finally` block always runs, even if an exception is raised.
///
/// ## Example
/// ```python
/// from contextlib import contextmanager
///
///
/// @contextmanager
/// def my_context():
///     print("setup")
///     yield
///     print("cleanup")  # This won't run if an exception is raised!
/// ```
///
/// Use instead:
/// ```python
/// from contextlib import contextmanager
///
///
/// @contextmanager
/// def my_context():
///     print("setup")
///     try:
///         yield
///     finally:
///         print("cleanup")  # This always runs
/// ```
///
/// ## References
/// - [Python documentation: `contextlib.contextmanager`](https://docs.python.org/3/library/contextlib.html#contextlib.contextmanager)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.14")]
pub(crate) struct ContextManagerWithoutFinally;

impl Violation for ContextManagerWithoutFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`yield` in context manager without `try`/`finally` is not protected against exceptions"
            .to_string()
    }
}

/// B036
pub(crate) fn contextmanager_without_finally(checker: &Checker, function_def: &StmtFunctionDef) {
    if !has_contextmanager_decorator(checker, function_def) {
        return;
    }

    let last_yield_range = function_def.body.last().and_then(|stmt| {
        if let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt
            && matches!(value.as_ref(), Expr::Yield(_) | Expr::YieldFrom(_))
        {
            Some(value.range())
        } else {
            None
        }
    });

    let mut visitor = YieldFinallyVisitor::default();
    visitor.visit_body(&function_def.body);

    for unprotected_yield in visitor.unprotected_yields {
        if Some(unprotected_yield) == last_yield_range {
            continue;
        }
        checker.report_diagnostic(ContextManagerWithoutFinally, unprotected_yield);
    }
}

fn has_contextmanager_decorator(checker: &Checker, function_def: &StmtFunctionDef) -> bool {
    function_def.decorator_list.iter().any(|decorator| {
        checker
            .semantic()
            .resolve_qualified_name(map_callable(&decorator.expression))
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
                    ["contextlib", "contextmanager" | "asynccontextmanager"]
                )
            })
    })
}

#[derive(Default)]
struct YieldFinallyVisitor {
    unprotected_yields: Vec<TextRange>,
    in_try_finally: bool,
    in_with_last_statement: bool,
}

impl Visitor<'_> for YieldFinallyVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}

            Stmt::Try(ast::StmtTry {
                body, finalbody, ..
            }) if !finalbody.is_empty() => {
                let prev = self.in_try_finally;
                self.in_try_finally = true;
                self.visit_body(body);
                self.in_try_finally = prev;
            }

            Stmt::With(ast::StmtWith { body, .. }) => {
                self.visit_with_body(body);
            }

            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Yield(_) | Expr::YieldFrom(_) => {
                if !self.in_try_finally && !self.in_with_last_statement {
                    self.unprotected_yields.push(expr.range());
                }
            }
            Expr::Lambda(_) => {}
            _ => walk_expr(self, expr),
        }
    }
}

impl YieldFinallyVisitor {
    fn visit_with_body(&mut self, body: &[Stmt]) {
        if body.is_empty() {
            return;
        }

        for stmt in &body[..body.len() - 1] {
            self.visit_stmt(stmt);
        }

        let last = &body[body.len() - 1];
        if Self::is_yield_statement(last) {
            let prev = self.in_with_last_statement;
            self.in_with_last_statement = true;
            self.visit_stmt(last);
            self.in_with_last_statement = prev;
        } else {
            self.visit_stmt(last);
        }
    }

    fn is_yield_statement(stmt: &Stmt) -> bool {
        matches!(
            stmt,
            Stmt::Expr(ast::StmtExpr { value, .. })
                if matches!(value.as_ref(), Expr::Yield(_) | Expr::YieldFrom(_))
        )
    }
}
