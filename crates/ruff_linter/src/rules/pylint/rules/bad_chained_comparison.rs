use itertools::Itertools;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for semantically different operators in a chained comparison
///
/// ## Why it this bad?
/// Combining semantically different operators in a chained comparison
/// can be misleading or a mistake.
///
/// ## Example
/// ```python
/// def xor_check(*, left=None, right=None):
///    if left is None != right is None:
///        raise ValueError(
///            "Either both left= and right= need to be provided or none should."
///        )
/// ```
///
/// Use instead:
/// ```python
/// def xor_check(*, left=None, right=None):
///    if (left is None) != (right is None):
///        raise ValueError(
///            "Either both left= and right= need to be provided or none should."
///        )
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct BadChainedComparison {
    operators: Vec<&'static str>,
}

impl Violation for BadChainedComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let count = self.operators.len();
        let operators = self.operators[0..(count - 1)].join("', '");
        let last = self.operators[count - 1];
        format!(
            "Suspicious {count}-part chained comparison using semantically incompatible operators ('{operators}' and '{last}')"
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CmpOpKind {
    Comparison,
    Identity,
    Membership,
}

impl From<&CmpOp> for CmpOpKind {
    fn from(op: &CmpOp) -> Self {
        match op {
            CmpOp::Eq | CmpOp::NotEq | CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE => {
                CmpOpKind::Comparison
            }
            CmpOp::Is | CmpOp::IsNot => CmpOpKind::Identity,
            CmpOp::In | CmpOp::NotIn => CmpOpKind::Membership,
        }
    }
}

/// W3601
pub(crate) fn bad_chained_comparison(checker: &Checker, expr: &Expr, ops: &[CmpOp]) {
    if ops.len() < 2 {
        return;
    }

    let kind = CmpOpKind::from(&ops[0]);
    for op in ops.iter().skip(1) {
        if kind != CmpOpKind::from(op) {
            let operators = ops
                .iter()
                .unique()
                .map(ruff_python_ast::CmpOp::as_str)
                .sorted()
                .collect();
            checker.report_diagnostic(BadChainedComparison { operators }, expr.range());
            return;
        }
    }
}
