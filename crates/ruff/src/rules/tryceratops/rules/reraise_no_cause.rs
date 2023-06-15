use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::RaiseStatementVisitor;
use ruff_python_ast::statement_visitor::StatementVisitor;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for exceptions that are re-raised without specifying the cause via
/// the `from` keyword.
///
/// ## Why is this bad?
/// The `from` keyword sets the `__cause__` attribute of the exception, which
/// stores the "cause" of the exception. The availability of an exception
/// "cause" is useful for debugging.
///
/// ## Example
/// ```python
/// def reciprocal(n):
///     try:
///         return 1 / n
///     except ZeroDivisionError:
///         raise ValueError
/// ```
///
/// Use instead:
/// ```python
/// def reciprocal(n):
///     try:
///         return 1 / n
///     except ZeroDivisionError as exc:
///         raise ValueError from exc
/// ```
///
/// ## References
/// - [Python documentation: Exception context](https://docs.python.org/3/library/exceptions.html#exception-context)
#[violation]
pub struct ReraiseNoCause;

impl Violation for ReraiseNoCause {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `raise from` to specify exception cause")
    }
}

/// TRY200
pub(crate) fn reraise_no_cause(checker: &mut Checker, body: &[Stmt]) {
    let raises = {
        let mut visitor = RaiseStatementVisitor::default();
        visitor.visit_body(body);
        visitor.raises
    };

    for (range, exc, cause) in raises {
        if exc.map_or(false, |expr| expr.is_call_expr() && cause.is_none()) {
            checker
                .diagnostics
                .push(Diagnostic::new(ReraiseNoCause, range));
        }
    }
}
