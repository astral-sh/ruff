use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::rules::is_open;

/// ## What it does
/// Checks for usages of `open` without a `with` statement.
///
/// ## Why is this bad?
/// When using `open` without a `with` statement, you must remember to close the file manually.
/// This can lead to resource leaks if the file is not closed properly.
/// Using a `with` statement ensures that the file is closed automatically when the block is exited, even if an exception is raised.
///
/// ## Example
///
/// ```python
/// f = open("file.txt", "r")
/// ```
///
/// Use instead:
///
/// ```python
/// with open("file.txt", "r") as f:
///     ...
/// ```
#[violation]
pub struct OpenWithoutWith;

impl Violation for OpenWithoutWith {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consider using `with open(...) as ...:` instead of `open(...)`")
    }
}

/// PLR1732
pub(crate) fn open_without_with(checker: &mut Checker, call: &ast::ExprCall) {
    let parent = checker.semantic().current_statement();
    if is_open(call.func.as_ref(), checker.semantic()).is_none() {
        return;
    };
    if let Some(with_stmt) = parent.as_with_stmt() {
        if with_stmt
            .items
            .iter()
            .any(|item| item.context_expr.range() == call.range())
        {
            return;
        }
    }
    checker
        .diagnostics
        .push(Diagnostic::new(OpenWithoutWith, call.range()));
}
