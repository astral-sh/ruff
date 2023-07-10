use rustpython_parser::ast::{self, ExceptHandler, Expr, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for needless exception names in `raise` statements.
///
/// ## Why is this bad?
/// It's redundant to specify the exception name in a `raise` statement if the
/// exception is being re-raised.
///
/// ## Example
/// ```python
/// def foo():
///     try:
///         ...
///     except ValueError as exc:
///         raise exc
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     try:
///         ...
///     except ValueError:
///         raise
/// ```
#[violation]
pub struct VerboseRaise;

impl Violation for VerboseRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `raise` without specifying exception name")
    }
}

#[derive(Default)]
struct RaiseStatementVisitor<'a> {
    raises: Vec<(Option<&'a Expr>, Option<&'a Expr>)>,
}

impl<'a, 'b> StatementVisitor<'b> for RaiseStatementVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match stmt {
            Stmt::Raise(ast::StmtRaise {
                exc,
                cause,
                range: _,
            }) => {
                self.raises.push((exc.as_deref(), cause.as_deref()));
            }
            Stmt::Try(ast::StmtTry {
                body, finalbody, ..
            })
            | Stmt::TryStar(ast::StmtTryStar {
                body, finalbody, ..
            }) => {
                for stmt in body.iter().chain(finalbody.iter()) {
                    walk_stmt(self, stmt);
                }
            }
            _ => walk_stmt(self, stmt),
        }
    }
}

/// TRY201
pub(crate) fn verbose_raise(checker: &mut Checker, handlers: &[ExceptHandler]) {
    for handler in handlers {
        // If the handler assigned a name to the exception...
        if let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            name: Some(exception_name),
            body,
            ..
        }) = handler
        {
            let raises = {
                let mut visitor = RaiseStatementVisitor::default();
                visitor.visit_body(body);
                visitor.raises
            };
            for (exc, cause) in raises {
                if cause.is_some() {
                    continue;
                }
                if let Some(exc) = exc {
                    // ...and the raised object is bound to the same name...
                    if let Expr::Name(ast::ExprName { id, .. }) = exc {
                        if id == exception_name.as_str() {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(VerboseRaise, exc.range()));
                        }
                    }
                }
            }
        }
    }
}
