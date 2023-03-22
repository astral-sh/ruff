use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `while` loop use
///
/// ## Why is this bad?
/// Unbounded `while` loops can often be rewritten as bounded `for` loops to avoid out of range or other similar errors
///
/// ## Example
/// ```python
/// i = 1
/// while i < 6:
///     print(i)
///     i = i + 1
/// ```
///
/// Use instead:
/// ```python
/// for i in range(1, 6):
///     print(i)
/// ```
#[violation]
pub struct WhileUsed;

impl Violation for WhileUsed {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Used `while` loop")
    }
}

/// PLW0149
pub fn while_used(checker: &mut Checker, stmt: &Stmt) {
    checker
        .diagnostics
        .push(Diagnostic::new(WhileUsed, Range::from(stmt)));
}
