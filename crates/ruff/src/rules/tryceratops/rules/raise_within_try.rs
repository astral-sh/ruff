use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, ExceptHandler, Expr, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    helpers,
    statement_visitor::{walk_stmt, StatementVisitor},
};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `raise` statements within `try` blocks. The only `raise`s
/// caught are those that throw exceptions caught by the `try` statement itself.
///
/// ## Why is this bad?
/// Raising and catching exceptions within the same `try` block is redundant,
/// as the code can be refactored to avoid the `try` block entirely.
///
/// Alternatively, the `raise` can be moved within an inner function, making
/// the exception reusable across multiple call sites.
///
/// ## Example
/// ```python
/// def bar():
///     pass
///
///
/// def foo():
///     try:
///         a = bar()
///         if not a:
///             raise ValueError
///     except ValueError:
///         raise
/// ```
///
/// Use instead:
/// ```python
/// def bar():
///     raise ValueError
///
///
/// def foo():
///     try:
///         a = bar()  # refactored `bar` to raise `ValueError`
///     except ValueError:
///         raise
/// ```
#[violation]
pub struct RaiseWithinTry;

impl Violation for RaiseWithinTry {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Abstract `raise` to an inner function")
    }
}

#[derive(Default)]
struct RaiseStatementVisitor<'a> {
    raises: Vec<&'a Stmt>,
}

impl<'a, 'b> StatementVisitor<'b> for RaiseStatementVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match stmt {
            Stmt::Raise(_) => self.raises.push(stmt),
            Stmt::Try(_) | Stmt::TryStar(_) => (),
            _ => walk_stmt(self, stmt),
        }
    }
}

/// TRY301
pub(crate) fn raise_within_try(checker: &mut Checker, body: &[Stmt], handlers: &[ExceptHandler]) {
    if handlers.is_empty() {
        return;
    }

    // The names of exceptions handled by the `try` Stmt. We can't compare `Expr`'s since they have
    // different ranges, but by virtue of this function's call path we know that the `raise`
    // statements will always be within the surrounding `try`.
    let handler_ids: FxHashSet<&str> = helpers::extract_handled_exceptions(handlers)
        .into_iter()
        .filter_map(|handler| {
            if let Expr::Name(ast::ExprName { id, .. }) = handler {
                Some(id.as_str())
            } else {
                None
            }
        })
        .collect();

    let raises = {
        let mut visitor = RaiseStatementVisitor::default();
        visitor.visit_body(body);
        visitor.raises
    };

    for stmt in raises {
        let Stmt::Raise(ast::StmtRaise { exc, .. }) = stmt else {
            continue;
        };

        let Some(exception) = exc else {
            continue;
        };

        let exc_name = get_function_name(exception.as_ref()).unwrap_or_default();
        // We can't check exception sub-classes without a type-checker implementation, so let's
        // just catch the blanket `Exception` for now.
        if handler_ids.contains(exc_name) || handler_ids.contains("Exception") {
            checker
                .diagnostics
                .push(Diagnostic::new(RaiseWithinTry, stmt.range()));
        }
    }
}

/// Get the name of an [`Expr::Call`], if applicable. If the passed [`Expr`] isn't a [`Expr::Call`], return an
/// empty [`Option`].
fn get_function_name(expr: &Expr) -> Option<&str> {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return None;
    };

    match func.as_ref() {
        Expr::Name(ast::ExprName { id, .. }) => Some(id.as_str()),
        _ => None,
    }
}
