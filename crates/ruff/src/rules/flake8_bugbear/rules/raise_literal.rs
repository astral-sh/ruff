use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for raising a literal.
///
/// ## Why is this bad?
/// Exceptions must be instances of `BaseException` or a subclass of it. Raising
/// a literal is not allowed and will raise a `TypeError`.
///
/// ## Example
/// ```python
/// raise "foo"
/// ```
///
/// Use instead:
/// ```python
/// raise Exception("foo")
/// ```
///
/// ## References
/// - [Python documentation: `raise` statement](https://docs.python.org/3/reference/simple_stmts.html#the-raise-statement)
#[violation]
pub struct RaiseLiteral;

impl Violation for RaiseLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Cannot raise a literal. Did you intend to return it or raise an Exception?")
    }
}

/// B016
pub(crate) fn raise_literal(checker: &mut Checker, expr: &Expr) {
    if expr.is_constant_expr() {
        checker
            .diagnostics
            .push(Diagnostic::new(RaiseLiteral, expr.range()));
    }
}
