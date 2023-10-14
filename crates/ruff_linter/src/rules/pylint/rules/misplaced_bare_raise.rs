use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Stmt;
use ruff_python_semantic::NodeRef;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
///
/// This rule triggers an error when a bare raise statement is not in an except or finally block.
///
/// ## Why is this bad?
///
/// If raise statement is not in an except or finally block, there is no active exception to
/// re-raise, so it will fail with a `RuntimeError` exception.
///
/// ## Example
/// ```python
/// def validate_positive(x):
///     if x <= 0:
///         raise
/// ```
///
/// Use instead:
/// ```python
/// def validate_positive(x):
///     if x <= 0:
///         raise ValueError(f"{x} is not positive")
/// ```
#[violation]
pub struct MisplacedBareRaise;

impl Violation for MisplacedBareRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The raise statement is not inside an except clause")
    }
}

/// PLE0704
pub(crate) fn misplaced_bare_raise(checker: &mut Checker, stmt: &Stmt) {
    if checker.semantic().in_exception_handler() {
        return;
    }
    for id in checker.semantic().current_statement_ids() {
        let node = checker.semantic().node(id);
        if let NodeRef::Stmt(Stmt::FunctionDef(fd)) = node {
            // allow bare raise in __exit__ methods
            if let Some(Stmt::ClassDef(_)) = checker.semantic().parent_statement(id) {
                if fd.name.as_str() == "__exit__" {
                    return;
                }
            }
            break;
        }
    }
    checker
        .diagnostics
        .push(Diagnostic::new(MisplacedBareRaise, stmt.range()));
}
