use ruff_python_ast::{Expr, StmtAssign};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for assignments with multiple or non-name targets in stub files.
///
/// ## Why is this bad?
/// In general, stub files should be thought of as "data files" for a type
/// checker, and are not intended to be executed. As such, it's useful to
/// enforce that only a subset of Python syntax is allowed in a stub file, to
/// ensure that everything in the stub is unambiguous for the type checker.
///
/// The need to perform multi-assignment, or assignment to a non-name target,
/// likely indicates a misunderstanding of how stub files are intended to be
/// used.
///
/// ## Example
///
/// ```pyi
/// from typing import TypeAlias
///
/// a = b = int
///
/// class Klass: ...
///
/// Klass.X: TypeAlias = int
/// ```
///
/// Use instead:
///
/// ```pyi
/// from typing import TypeAlias
///
/// a: TypeAlias = int
/// b: TypeAlias = int
///
/// class Klass:
///     X: TypeAlias = int
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
