use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{self as ast, Expr, Stmt, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `@contextlib.contextmanager` decorated functions that contain
/// `yield` expressions not protected against exceptions.
///
/// ## Why is this bad?
/// When a context manager is used, code after `yield` is intended to run
/// during cleanup when the context exits. However, if an exception is raised
/// inside the `with` block, code after an unprotected `yield` will never
/// execute.
///
/// Wrapping `yield` in a `try`/`finally` or `try`/`except` block ensures
/// that exceptions are handled appropriately.
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

    let mut visitor = YieldFinallyVisitor::default();
    visitor.visit_body_with_terminal(&function_def.body, true);

    for unprotected_yield in visitor.unprotected_yields {
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
    in_protected_try: bool,
    in_with_last_statement: bool,
    in_terminal_position: bool,
}

impl Visitor<'_> for YieldFinallyVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}

            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) if !finalbody.is_empty() || !handlers.is_empty() => {
                let prev = self.in_protected_try;
                self.in_protected_try = true;
                self.visit_body(body);
                for handler in handlers {
                    self.visit_except_handler(handler);
                }
                self.visit_body(orelse);
                self.in_protected_try = prev;
                self.visit_body(finalbody);
            }

            Stmt::With(ast::StmtWith { body, .. }) => {
                self.visit_with_body(body);
            }

            Stmt::If(ast::StmtIf {
                test,
                body,
                elif_else_clauses,
                ..
            }) => {
                self.visit_expr(test);
                let terminal = self.in_terminal_position;
                self.visit_body_with_terminal(body, terminal);
                for clause in elif_else_clauses {
                    if let Some(test) = &clause.test {
                        self.visit_expr(test);
                    }
                    self.visit_body_with_terminal(&clause.body, terminal);
                }
            }

            Stmt::For(ast::StmtFor {
                target,
                iter,
                body,
                orelse,
                ..
            }) => {
                self.visit_expr(iter);
                self.visit_expr(target);
                let terminal = self.in_terminal_position;
                self.visit_body_with_terminal(body, terminal);
                self.visit_body_with_terminal(orelse, terminal);
            }

            Stmt::While(ast::StmtWhile {
                test, body, orelse, ..
            }) => {
                self.visit_expr(test);
                let terminal = self.in_terminal_position;
                self.visit_body_with_terminal(body, terminal);
                self.visit_body_with_terminal(orelse, terminal);
            }

            Stmt::Match(ast::StmtMatch { subject, cases, .. }) => {
                self.visit_expr(subject);
                let terminal = self.in_terminal_position;
                for case in cases {
                    if let Some(guard) = &case.guard {
                        self.visit_expr(guard);
                    }
                    self.visit_body_with_terminal(&case.body, terminal);
                }
            }

            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Yield(_) | Expr::YieldFrom(_) => {
                if !self.in_protected_try
                    && !self.in_with_last_statement
                    && !self.in_terminal_position
                {
                    self.unprotected_yields.push(expr.range());
                }
            }
            Expr::Lambda(_) => {}
            _ => walk_expr(self, expr),
        }
    }
}

impl YieldFinallyVisitor {
    fn visit_body_with_terminal(&mut self, body: &[Stmt], terminal: bool) {
        for (i, stmt) in body.iter().enumerate() {
            let is_last = i == body.len() - 1;
            let is_yield_before_return = Self::is_yield_statement(stmt)
                && body
                    .get(i + 1)
                    .is_some_and(|next| matches!(next, Stmt::Return(_)));

            let prev = self.in_terminal_position;
            self.in_terminal_position = (is_last && terminal) || is_yield_before_return;
            self.visit_stmt(stmt);
            self.in_terminal_position = prev;
        }
    }

    fn visit_with_body(&mut self, body: &[Stmt]) {
        if body.is_empty() {
            return;
        }

        let parent_terminal = self.in_terminal_position;

        // Non-last statements: delegate terminal tracking to visit_body_with_terminal
        if body.len() > 1 {
            self.visit_body_with_terminal(&body[..body.len() - 1], false);
        }

        // Last statement: inherit terminal from parent, set with-last-statement if yield
        let last = &body[body.len() - 1];
        let prev_terminal = self.in_terminal_position;
        self.in_terminal_position = parent_terminal;

        if Self::is_yield_statement(last) {
            let prev_with = self.in_with_last_statement;
            self.in_with_last_statement = true;
            self.visit_stmt(last);
            self.in_with_last_statement = prev_with;
        } else {
            self.visit_stmt(last);
        }

        self.in_terminal_position = prev_terminal;
    }

    fn is_yield_statement(stmt: &Stmt) -> bool {
        matches!(
            stmt,
            Stmt::Expr(ast::StmtExpr { value, .. })
                if matches!(value.as_ref(), Expr::Yield(_) | Expr::YieldFrom(_))
        )
    }
}
