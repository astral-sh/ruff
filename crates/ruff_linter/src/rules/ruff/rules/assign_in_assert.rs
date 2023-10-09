use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for assignment expressions in assert statements.
///
/// ## Why is this bad?
/// Assignment expressions in assert statements will not be executed when the
/// Python interpreter is run with the `-O` option.
///
/// ## Examples
/// ```python
/// assert (x := 0) == 0
/// ```
///
/// Use instead:
/// ```python
/// x = 0
/// assert x == 0
/// ```
///
/// ## References
/// - [Python documentation: command option `-O`](https://docs.python.org/3/using/cmdline.html#cmdoption-O)
#[violation]
pub struct AssignInAssert;

impl Violation for AssignInAssert {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assignment expression in assert statement is not allowed")
    }
}

/// RUF018
pub(crate) fn assign_in_assert(checker: &mut Checker, value: &Expr) {
    if checker.semantic().in_assert() {
        checker
            .diagnostics
            .push(Diagnostic::new(AssignInAssert, value.range()));
    }
}
