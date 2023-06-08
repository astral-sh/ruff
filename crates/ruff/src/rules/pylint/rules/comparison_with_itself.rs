use itertools::Itertools;
use rustpython_parser::ast::{Cmpop, Expr};

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::ViolationsCmpop;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for comparisons of a name with itself.
///
/// ## Why is this bad?
/// Comparing a name with itself will always resolve to the same value, so the
/// comparison is redundant. It is also indicative of a mistake, as the
/// comparison is unlikely to be what the programmer intended.
///
/// ## Example
/// ```python
/// foo == foo
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/expressions.html#comparisons)
#[violation]
pub struct ComparisonWithItself {
    left_constant: String,
    op: ViolationsCmpop,
    right_constant: String,
}

impl Violation for ComparisonWithItself {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ComparisonWithItself {
            left_constant,
            op,
            right_constant,
        } = self;

        format!(
            "Name compared with itself, consider replacing `{left_constant} {op} \
             {right_constant}`"
        )
    }
}

/// PLR0124
pub(crate) fn comparison_with_itself(
    checker: &mut Checker,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    for ((left, right), op) in std::iter::once(left)
        .chain(comparators.iter())
        .tuple_windows()
        .zip(ops)
    {
        if let (Expr::Name(left_expr), Expr::Name(right_expr)) = (left, right) {
            if left_expr.id == right_expr.id {
                let diagnostic = Diagnostic::new(
                    ComparisonWithItself {
                        left_constant: left_expr.id.to_string(),
                        op: op.into(),
                        right_constant: right_expr.id.to_string(),
                    },
                    left_expr.range,
                );

                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
