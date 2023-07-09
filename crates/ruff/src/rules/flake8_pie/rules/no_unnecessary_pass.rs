use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::AlwaysAutofixableViolation;
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{is_docstring_stmt, trailing_comment_start_offset};

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for unnecessary `pass` statements in class and function bodies.
/// where it is not needed syntactically (e.g., when an indented docstring is
/// present).
///
/// ## Why is this bad?
/// When a function or class definition contains a docstring, an additional
/// `pass` statement is redundant.
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

impl AlwaysAutofixableViolation for UnnecessaryPass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `pass` statement")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary `pass`".to_string()
    }
}

/// PIE790
pub(crate) fn no_unnecessary_pass(checker: &mut Checker, body: &[Stmt]) {
    if body.len() > 1 {
        // This only catches the case in which a docstring makes a `pass` statement
        // redundant. Consider removing all `pass` statements instead.
        if !is_docstring_stmt(&body[0]) {
            return;
        }

        // The second statement must be a `pass` statement.
        let stmt = &body[1];
        if !stmt.is_pass_stmt() {
            return;
        }

        let mut diagnostic = Diagnostic::new(UnnecessaryPass, stmt.range());
        if checker.patch(diagnostic.kind.rule()) {
            let edit = if let Some(index) = trailing_comment_start_offset(stmt, checker.locator) {
                Edit::range_deletion(stmt.range().add_end(index))
            } else {
                autofix::edits::delete_stmt(stmt, None, checker.locator, checker.indexer)
            };
            diagnostic.set_fix(Fix::automatic(edit));
        }
        checker.diagnostics.push(diagnostic);
    }
}
