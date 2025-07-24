use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{ExprBinOp, Operator};

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for binary operations between a value and itself with known identities.
///
/// ## Why is this bad?
/// Binary operations between a value and itself can be simplified to known identities,
/// making the code more readable and potentially more efficient.
///
/// ## Example
/// ```python
/// x | x  # => x
/// x & x  # => x
/// x ^ x  # => 0
/// x - x  # => 0
/// x / x  # => 1
/// x // x # => 1
/// x % x  # => 0
/// ```
///
/// Use instead:
/// ```python
/// x
/// x
/// 0
/// 0
/// 1
/// 1
/// 0
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct BinaryOperatorIdentity {
    operator: Operator,
    identity: String,
}

impl AlwaysFixableViolation for BinaryOperatorIdentity {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Binary operation `{}` between a value and itself can be simplified to `{}`",
            self.operator, self.identity
        )
    }

    fn fix_title(&self) -> String {
        format!("Replace with `{}`", self.identity)
    }
}

/// RUF065
pub(crate) fn binary_operator_identity(checker: &Checker, bin_op: &ExprBinOp) {
    let ExprBinOp {
        left,
        op,
        right,
        range,
        ..
    } = bin_op;

    // Check if left and right operands are identical
    if ComparableExpr::from(left) != ComparableExpr::from(right) {
        return;
    }

    // Skip boolean operations - they are logical operations, not identity simplifications
    if left.as_boolean_literal_expr().is_some() {
        return;
    }

    // Determine the identity value based on the operator
    let identity = match op {
        Operator::BitOr | Operator::BitAnd => checker.locator().slice(left.as_ref()).to_string(),
        Operator::BitXor | Operator::Sub | Operator::Mod => "0".to_string(),
        Operator::Div | Operator::FloorDiv => "1".to_string(),
        _ => {
            return;
        }
    };

    let mut diagnostic = checker.report_diagnostic(
        BinaryOperatorIdentity {
            operator: *op,
            identity: identity.clone(),
        },
        *range,
    );

    let edit = Edit::range_replacement(identity, *range);
    diagnostic.set_fix(Fix::safe_edit(edit));
}
