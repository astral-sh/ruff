use itertools::Itertools;
use ruff_macros::{derive_message_formats, violation};
use rustpython_parser::ast::{Cmpop, Expr, ExprKind, Located};

use crate::ast::helpers::unparse_constant;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

#[violation]
pub struct CompareToEmptyString;

impl Violation for CompareToEmptyString {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("todo")
    }
}

pub fn compare_to_empty_string(
    checker: &mut Checker,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    for ((left, rhs), op) in std::iter::once(left)
        .chain(comparators.iter())
        .tuple_windows::<(&Located<_>, &Located<_>)>()
        .zip(ops)
    {
        if matches!(op, Cmpop::Eq | Cmpop::NotEq) {
            if let ExprKind::Constant { value: v, .. } = &rhs.node {
                let k = unparse_constant(v, checker.stylist);
                if k == "\"\"" || k == "''" {
                    let diag = Diagnostic::new(CompareToEmptyString {}, Range::from_located(left));
                    checker.diagnostics.push(diag);
                }
            }
        }
    }
}
