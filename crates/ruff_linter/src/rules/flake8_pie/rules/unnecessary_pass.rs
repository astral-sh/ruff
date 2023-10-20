use ruff_python_ast::Stmt;

use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::whitespace::trailing_comment_start_offset;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;

/// ## What it does
/// Checks for unnecessary `pass` statements in functions, classes, and other
/// blocks.
///
/// ## Why is this bad?
/// In Python, the `pass` statement serves as a placeholder, allowing for
/// syntactically correct empty code blocks. The primary purpose of the `pass`
/// statement is to avoid syntax errors in situations where a statement is
/// syntactically required, but no code needs to be executed.
///
/// If a `pass` statement is present in a code block that includes at least
/// one other statement (even, e.g., a docstring), it is unnecessary and should
/// be removed.
///
/// ## Example
/// ```python
/// def func():
///     """Placeholder docstring."""
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def func():
///     """Placeholder docstring."""
/// ```
///
/// ## References
/// - [Python documentation: The `pass` statement](https://docs.python.org/3/reference/simple_stmts.html#the-pass-statement)
#[violation]
pub struct UnnecessaryPass;

impl AlwaysFixableViolation for UnnecessaryPass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `pass` statement")
    }

    fn fix_title(&self) -> String {
        "Remove unnecessary `pass`".to_string()
    }
}

/// PIE790
pub(crate) fn no_unnecessary_pass(checker: &mut Checker, body: &[Stmt]) {
    if body.len() < 2 {
        return;
    }

    body.iter()
        .filter(|stmt| stmt.is_pass_stmt())
        .for_each(|stmt| {
            let mut diagnostic = Diagnostic::new(UnnecessaryPass, stmt.range());
            let edit = if let Some(index) = trailing_comment_start_offset(stmt, checker.locator()) {
                Edit::range_deletion(stmt.range().add_end(index))
            } else {
                fix::edits::delete_stmt(stmt, None, checker.locator(), checker.indexer())
            };
            diagnostic.set_fix(Fix::safe_edit(edit).isolate(Checker::isolation(
                checker.semantic().current_statement_id(),
            )));
            checker.diagnostics.push(diagnostic);
        });
}
