use rustpython_parser::ast::{Expr, StmtAssign};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for assignments with multiple or non-name targets
///
/// ## Why is this bad?
/// Stub files are not intended to ever be executed. As such, it's useful to enforce that only a
/// subset of Python syntax is allowed in a stub file, to ensure that everything in the stub is
/// 100% unambiguous when it comes to how the type checker is supposed to interpret it. Only
/// allowing simple assignments is one such restriction.
///
/// ## Example
/// ```python
/// a = b = int
/// a.b = int
/// ```
///
/// Use instead:
/// ```python
/// a: TypeAlias = int
/// b: TypeAlias = int
///
///
/// class a:
///     b: int
/// ```
#[violation]
pub struct ComplexAssignmentInStub;

impl Violation for ComplexAssignmentInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Stubs should not contain assignments to attributes or multiple targets")
    }
}

/// PYI017
pub(crate) fn complex_assignment_in_stub(checker: &mut Checker, stmt: &StmtAssign) {
    if matches!(stmt.targets.as_slice(), [Expr::Name(_)]) {
        return;
    }
    checker
        .diagnostics
        .push(Diagnostic::new(ComplexAssignmentInStub, stmt.range));
}
