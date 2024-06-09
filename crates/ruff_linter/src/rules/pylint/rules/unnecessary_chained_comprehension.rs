use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};

/// ## What it does
/// Checks for boolean operations such as `a < b and b < c`
/// that can be refactored into a single comparison `a < b < c`.
///
/// ## Why is this bad?
/// A single comparison is semantically clearer and reduces the total
/// amount of expressions.
///
/// ## Example
/// ```python
/// a = int(input())
/// b = int(input())
/// c = int(input())
/// if a < b and b < c
///     pass
/// ```
///
/// Use instead:
/// ```python
/// a = int(input())
/// b = int(input())
/// c = int(input())
/// if a < b < c
///     pass
/// ```

#[violation]
pub struct UnnecessaryChainedComprehension;

impl Violation for UnnecessaryChainedComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Simplified chain comparision exists between the operands.")
    }
}

/// PLC1716
pub(crate) fn unnecessary_chained_comprehension(checker: &mut Checker, bool_op: &ast::ExprBoolOp) {
    ..
}
