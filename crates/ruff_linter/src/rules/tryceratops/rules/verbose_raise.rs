use ruff_python_ast::{self as ast, ExceptHandler, Expr, Stmt};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use ruff_text_size::Ranged;

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

impl AlwaysFixableViolation for VerboseRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `raise` without specifying exception name")
    }

    fn fix_title(&self) -> String {
        format!("Remove exception name")
    }
}

#[derive(Default)]
struct RaiseStatementVisitor<'a> {
    raises: Vec<&'a ast::StmtRaise>,
}

impl<'a, 'b> StatementVisitor<'b> for RaiseStatementVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match stmt {
            Stmt::Raise(raise @ ast::StmtRaise { .. }) => {
                self.raises.push(raise);
            }
            Stmt::Try(ast::StmtTry {
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
            for raise in raises {
                if raise.cause.is_some() {
                    continue;
                }
                if let Some(exc) = raise.exc.as_ref() {
                    // ...and the raised object is bound to the same name...
                    if let Expr::Name(ast::ExprName { id, .. }) = exc.as_ref() {
                        if id == exception_name.as_str() {
                            let mut diagnostic = Diagnostic::new(VerboseRaise, exc.range());
                            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                                "raise".to_string(),
                                raise.range(),
                            )));
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                }
            }
        }
    }
}
