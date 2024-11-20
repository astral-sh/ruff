use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::StmtAssert;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of non-string literal as assert message.
///
/// ## Why is this bad?
/// Non-string literal in assert message does not provide any useful
/// information and is likely an unitentional use of `assert_equal(expr, expr)`
/// from other languages.
///
/// ## Example
/// ```python
/// fruits = ["apples", "plums", "pears"]
/// fruits.filter(lambda fruit: fruit.startwith("p"))
/// assert len(fruits), 2  # True unless the list is empty
/// ```
///
/// Use instead:
/// ```python
/// fruits = ["apples", "plums", "pears"]
/// fruits.filter(lambda fruit: fruit.startwith("p"))
/// assert len(fruits) == 2
/// ```
#[violation]
pub struct NonStringLiteralAsAssertMessage;

impl Violation for NonStringLiteralAsAssertMessage {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Non-string literal used as assert message".to_string()
    }
}

/// RUF035
pub(crate) fn non_string_literal_as_assert_message(checker: &mut Checker, stmt: &StmtAssert) {
    let Some(message) = stmt.msg.as_deref() else {
        return;
    };
    if message.is_string_literal_expr() || !message.is_literal_expr() {
        return;
    }
    checker.diagnostics.push(Diagnostic::new(
        NonStringLiteralAsAssertMessage,
        message.range(),
    ));
}
