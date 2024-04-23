use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::rules::is_open;

/// ## What it does
/// Checks for hard-coded sequence accesses that are known to be out of bounds.
///
/// ## Why is this bad?
/// Attempting to access a sequence with an out-of-bounds index will cause an
/// `IndexError` to be raised at runtime. When the sequence and index are
/// defined statically (e.g., subscripts on `list` and `tuple` literals, with
/// integer indexes), such errors can be detected ahead of time.
///
/// ## Example
/// ```python
/// print([0, 1, 2][3])
/// ```
#[violation]
pub struct OpenWithoutWith;

impl Violation for OpenWithoutWith {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consider using `with open(...) as ...:` instead of `open(...)`")
    }
}

/// PLE0643
pub(crate) fn open_without_with(checker: &mut Checker, call: &ast::ExprCall) {
    let parent = checker.semantic().current_statement();
    let Some(kind) = is_open(call.func.as_ref(), checker.semantic()) else {
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
