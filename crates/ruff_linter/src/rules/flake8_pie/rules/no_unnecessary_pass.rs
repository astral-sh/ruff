use ruff_python_ast::Stmt;

use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::whitespace::trailing_comment_start_offset;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;
use crate::registry::AsRule;

/// ## What it does
/// Checks for unnecessary `pass` statements in class and function bodies.
/// where it is not needed syntactically (e.g., when an indented docstring is
/// present).
///
/// ## Why is this bad?
/// When a function or class definition contains more than one statement, the
/// `pass` statement is not needed.
///
/// ## Example
/// ```python
/// def foo():
///     """Placeholder docstring."""
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def foo():
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
            if checker.patch(diagnostic.kind.rule()) {
                let edit =
                    if let Some(index) = trailing_comment_start_offset(stmt, checker.locator()) {
                        Edit::range_deletion(stmt.range().add_end(index))
                    } else {
                        fix::edits::delete_stmt(stmt, None, checker.locator(), checker.indexer())
                    };
                diagnostic.set_fix(Fix::automatic(edit));
            }
            checker.diagnostics.push(diagnostic);
        });
}
