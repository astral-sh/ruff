use rustpython_parser::ast::{self, Ranged};
use rustpython_parser::ast::{Excepthandler, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `except` clauses with an empty tuple.
///
/// ## Why is this bad?
/// Using `except` with an empty tuple does not catch anything. This is likely
/// a mistake. Instead, add exceptions to the `except` clause.
///
/// ## Example
/// ```python
/// try:
///     1 / 0
/// except ():
///     ...
/// ```
///
/// Use instead:
/// ```python
/// try:
///     1 / 0
/// except ZeroDivisionError:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `except` clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
#[violation]
pub struct ExceptWithEmptyTuple;

impl Violation for ExceptWithEmptyTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `except ():` with an empty tuple does not catch anything; add exceptions to handle")
    }
}

/// B029
pub(crate) fn except_with_empty_tuple(checker: &mut Checker, excepthandler: &Excepthandler) {
    let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler { type_, .. }) = excepthandler;
    let Some(type_) = type_ else {
        return;
    };
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = type_.as_ref() else {
        return;
    };
    if elts.is_empty() {
        checker
            .diagnostics
            .push(Diagnostic::new(ExceptWithEmptyTuple, excepthandler.range()));
    }
}
