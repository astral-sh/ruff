use itertools::Itertools;
use rustpython_parser::ast::{CmpOp, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::CmpOpExt;

/// ## What it does
/// Checks for operations that compare a name to itself.
///
/// ## Why is this bad?
/// Comparing a name to itself always results in the same value, and is likely
/// a mistake.
///
/// ## Example
/// ```python
/// foo == foo
/// ```
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
#[violation]
pub struct ComparisonWithItself {
    left: String,
    op: CmpOp,
    right: String,
}

impl Violation for ComparisonWithItself {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ComparisonWithItself { left, op, right } = self;
        format!(
            "Name compared with itself, consider replacing `{left} {} {right}`",
            CmpOpExt::from(op)
        )
    }
}

/// PLR0124
pub(crate) fn comparison_with_itself(
    checker: &mut Checker,
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
) {
    for ((left, right), op) in std::iter::once(left)
        .chain(comparators.iter())
        .tuple_windows()
        .zip(ops)
    {
        if let (Expr::Name(left), Expr::Name(right)) = (left, right) {
            if left.id == right.id {
                checker.diagnostics.push(Diagnostic::new(
                    ComparisonWithItself {
                        left: left.id.to_string(),
                        op: *op,
                        right: right.id.to_string(),
                    },
                    left.range(),
                ));
            }
        }
    }
}
