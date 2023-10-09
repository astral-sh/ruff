use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for named assignment expressions (e.g., `x := 0`) in `assert`
/// statements.
///
/// ## Why is this bad?
/// Named assignment expressions (also known as "walrus operators") are used to
/// assign a value to a variable as part of a larger expression.
///
/// Named assignments are syntactically valid in `assert` statements. However,
/// when the Python interpreter is run under the `-O` flag, `assert` statements
/// are not executed. In this case, the named assignment will also be ignored,
/// which may result in unexpected behavior (e.g., undefined variable
/// accesses).
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
/// - [Python documentation: `-O`](https://docs.python.org/3/using/cmdline.html#cmdoption-O)
#[violation]
pub struct AssignmentInAssert;

impl Violation for AssignmentInAssert {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid assignment expressions in `assert` statements")
    }
}

/// RUF018
pub(crate) fn assignment_in_assert(checker: &mut Checker, value: &Expr) {
    if checker.semantic().current_statement().is_assert_stmt() {
        checker
            .diagnostics
            .push(Diagnostic::new(AssignmentInAssert, value.range()));
    }
}
