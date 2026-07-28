use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{self as ast, Expr, Stmt, StmtFunctionDef};
use ruff_text_size::Ranged;

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
#[violation_metadata(preview_since = "0.15.14")]
pub(crate) struct FallibleContextManager;

impl Violation for FallibleContextManager {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Context manager does not catch exceptions".to_string()
    }
}

/// RUF075
pub(crate) fn fallible_context_manager(checker: &Checker, function_def: &StmtFunctionDef) {
    if !has_contextmanager_decorator(checker, function_def) {
        return;
    }

    let mut visitor = YieldFinallyVisitor::new(checker);
    visitor.visit_body_with_terminal(&function_def.body, true);
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

/// Visits the body of a `@contextmanager` function to find unprotected `yield` statements.
///
/// A `yield` is considered protected (and not flagged) if any of the following hold:
/// - It is inside a `try` block that has `finally` or `except` handlers.
/// - It is in a terminal position within a `with` block body (the context manager's
///   `__exit__` handles cleanup).
/// - It is in a terminal position (last statement in the function body, immediately
///   followed by `return`, or immediately followed by `break` from a loop that itself
///   sits in a terminal position), meaning there is no cleanup code that could be skipped.
struct YieldFinallyVisitor<'a, 'b> {
    /// The checker used to emit diagnostics.
    checker: &'a Checker<'b>,
    /// Whether the visitor is currently inside a `try` block that has
    /// `finally` or `except` handlers.
    in_protected_try: bool,
    /// Whether the visitor is at a terminal position: the last statement in
    /// the function body, a `yield` immediately before a `return`, or a `yield`
    /// immediately before a `break` from a terminal loop.
    in_terminal_position: bool,
    /// Whether a `break` from the innermost enclosing loop would exit to
    /// a terminal position.
    in_terminal_loop: bool,
}

impl<'a, 'b> YieldFinallyVisitor<'a, 'b> {
    /// Creates a new visitor with the given checker.
    fn new(checker: &'a Checker<'b>) -> Self {
        Self {
            checker,
            in_protected_try: false,
            in_terminal_position: false,
            in_terminal_loop: false,
        }
    }
}

impl Visitor<'_> for YieldFinallyVisitor<'_, '_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}

            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                // Only the `try` body itself catches exceptions. Yields in
                // `except` / `else` / `finally` are unprotected, but they
                // inherit the surrounding terminal position because the
                // `try` statement itself sits in that slot.
                let prev = self.in_protected_try;
                let terminal = self.in_terminal_position;
                self.in_protected_try = true;
                self.visit_body(body);
                self.in_protected_try = prev;
                for handler in handlers {
                    self.visit_except_handler_with_terminal(handler, terminal);
                }
                self.visit_body_with_terminal(orelse, terminal);
                self.visit_body_with_terminal(finalbody, terminal);
            }

            Stmt::With(ast::StmtWith { items, body, .. }) => {
                for item in items {
                    self.visit_expr(&item.context_expr);
                }
                self.visit_body_with_terminal(body, true);
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
                let prev_loop = self.in_terminal_loop;
                self.in_terminal_loop = terminal;
                self.visit_body_with_terminal(body, terminal);
                self.in_terminal_loop = prev_loop;
                self.visit_body_with_terminal(orelse, terminal);
            }

            Stmt::While(ast::StmtWhile {
                test, body, orelse, ..
            }) => {
                self.visit_expr(test);
                let terminal = self.in_terminal_position;
                let prev_loop = self.in_terminal_loop;
                self.in_terminal_loop = terminal;
                self.visit_body_with_terminal(body, terminal);
                self.in_terminal_loop = prev_loop;
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
                if !self.in_protected_try && !self.in_terminal_position {
                    self.checker
                        .report_diagnostic(FallibleContextManager, expr.range());
                }
            }
            Expr::Lambda(_) => {}
            _ => walk_expr(self, expr),
        }
    }
}

impl YieldFinallyVisitor<'_, '_> {
    /// Visits an `except` handler, propagating the surrounding `terminal` flag into its body.
    fn visit_except_handler_with_terminal(&mut self, handler: &ast::ExceptHandler, terminal: bool) {
        let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            type_, body, ..
        }) = handler;

        if let Some(type_) = type_ {
            self.visit_expr(type_);
        }
        self.visit_body_with_terminal(body, terminal);
    }

    /// Visits each statement in `body`, tracking whether each is in a terminal position.
    ///
    /// A statement is considered terminal if it is the last in the body (when `terminal` is true),
    /// if it is a yield statement immediately followed by a `return`, or if it is a yield
    /// statement immediately followed by a `break` from a terminal loop.
    fn visit_body_with_terminal(&mut self, body: &[Stmt], terminal: bool) {
        for (i, stmt) in body.iter().enumerate() {
            let is_last = i == body.len() - 1;
            let is_yield_before_terminator = Self::is_yield_statement(stmt)
                && body.get(i + 1).is_some_and(|next| match next {
                    Stmt::Return(_) => true,
                    Stmt::Break(_) => self.in_terminal_loop,
                    _ => false,
                });

            let prev = self.in_terminal_position;
            self.in_terminal_position = (is_last && terminal) || is_yield_before_terminator;
            self.visit_stmt(stmt);
            self.in_terminal_position = prev;
        }
    }

    /// Returns `true` if the statement is an expression statement containing a `yield` or `yield from`.
    fn is_yield_statement(stmt: &Stmt) -> bool {
        matches!(
            stmt,
            Stmt::Expr(ast::StmtExpr { value, .. })
                if matches!(value.as_ref(), Expr::Yield(_) | Expr::YieldFrom(_))
        )
    }
}
