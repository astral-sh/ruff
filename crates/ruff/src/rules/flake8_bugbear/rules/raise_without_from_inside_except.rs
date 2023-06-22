use rustpython_parser::ast::{self, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::RaiseStatementVisitor;
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_stdlib::str::is_cased_lowercase;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `raise` statements in exception handlers that lack a `from`
/// clause.
///
/// ## Why is this bad?
/// In Python, `raise` can be used with or without an exception from which the
/// current exception is derived. This is known as exception chaining. When
/// printing the stack trace, chained exceptions are displayed in such a way
/// so as make it easier to trace the exception back to its root cause.
///
/// When raising an exception from within an `except` clause, always include a
/// `from` clause to facilitate exception chaining. If the exception is not
/// chained, it will be difficult to trace the exception back to its root cause.
///
/// ## Example
/// ```python
/// try:
///     ...
/// except FileNotFoundError:
///     if ...:
///         raise RuntimeError("...")
///     else:
///         raise UserWarning("...")
/// ```
///
/// Use instead:
/// ```python
/// try:
///     ...
/// except FileNotFoundError as exc:
///     if ...:
///         raise RuntimeError("...") from None
///     else:
///         raise UserWarning("...") from exc
/// ```
///
/// ## References
/// - [Python documentation: `raise` statement](https://docs.python.org/3/reference/simple_stmts.html#the-raise-statement)
#[violation]
pub struct RaiseWithoutFromInsideExcept;

impl Violation for RaiseWithoutFromInsideExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Within an `except` clause, raise exceptions with `raise ... from err` or `raise ... \
             from None` to distinguish them from errors in exception handling"
        )
    }
}

/// B904
pub(crate) fn raise_without_from_inside_except(checker: &mut Checker, body: &[Stmt]) {
    let raises = {
        let mut visitor = RaiseStatementVisitor::default();
        visitor.visit_body(body);
        visitor.raises
    };

    for (range, exc, cause) in raises {
        if cause.is_none() {
            if let Some(exc) = exc {
                match exc {
                    Expr::Name(ast::ExprName { id, .. }) if is_cased_lowercase(id) => {}
                    _ => {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(RaiseWithoutFromInsideExcept, range));
                    }
                }
            }
        }
    }
}
