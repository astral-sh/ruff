use rustpython_parser::ast::{self, Expr, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for if tests that are non-empty tuples.
///
/// ## Why is this bad?
/// If tests are boolean expressions. Non-empty tuples are always `True`. This
/// means that the if statement will always run, which is likely a mistake.
///
/// ## Example
/// ```python
/// if (False,):
///     print("This will always run")
/// ```
///
/// Use instead:
/// ```python
/// if False:
///     print("This will never run")
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/compound_stmts.html#the-if-statement)
#[violation]
pub struct IfTuple;

impl Violation for IfTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("If test is a tuple, which is always `True`")
    }
}

/// F634
pub(crate) fn if_tuple(checker: &mut Checker, stmt: &Stmt, test: &Expr) {
    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = &test {
        if !elts.is_empty() {
            checker
                .diagnostics
                .push(Diagnostic::new(IfTuple, stmt.range()));
        }
    }
}
