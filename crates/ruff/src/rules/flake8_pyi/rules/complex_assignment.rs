use rustpython_parser::ast::{Expr, StmtAssign};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for assignments with multiple or non-name targets
///
/// ## Why is this bad?
/// TODO
///
/// ## Example
/// ```python
/// a = b = int
/// a.b = int
/// ```
///
/// Use instead:
/// ```python
/// a = int
/// b = int
///
/// TODO
///
/// ```
#[violation]
pub struct ComplexAssignment;

impl Violation for ComplexAssignment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Stubs should not contain assignments with multiple targets or non-name targets.")
    }
}

/// PYI017
pub(crate) fn complex_assignment(checker: &mut Checker, stmt: &StmtAssign) {
    if let [Expr::Name(_)] = stmt.targets[..] {
        return;
    }
    checker
        .diagnostics
        .push(Diagnostic::new(ComplexAssignment, stmt.range));
}
