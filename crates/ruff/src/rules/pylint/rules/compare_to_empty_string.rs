use itertools::Itertools;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_constant;
use ruff_python_ast::helpers::unparse_expr;
use ruff_python_ast::types::Range;
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind, Located};

use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

use super::comparison_of_constant::ViolationsCmpop;

#[violation]
pub struct CompareToEmptyString {
    pub lhs: String,
    pub op: ViolationsCmpop,
    pub rhs: String,
}

impl Violation for CompareToEmptyString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let cond = match self.op {
            ViolationsCmpop::Is | ViolationsCmpop::Eq => "",
            // the assumption is that this message will only ever be shown
            // if op is `in`, `=`, `not in`, `!=`
            _ => "not",
        };
        format!(
            "{} {} {} can be simplified to {} {} as strings are falsey",
            self.lhs, self.op, self.rhs, cond, self.lhs
        )
    }
}

pub fn compare_to_empty_string(
    checker: &mut Checker,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    for ((lhs, rhs), op) in std::iter::once(left)
        .chain(comparators.iter())
        .tuple_windows::<(&Located<_>, &Located<_>)>()
        .zip(ops)
    {
        if matches!(op, Cmpop::Eq | Cmpop::NotEq | Cmpop::Is | Cmpop::IsNot) {
            if let ExprKind::Constant { value, .. } = &rhs.node {
                if let Constant::Str(s) = value {
                    if s.is_empty() {
                        let diag = Diagnostic::new(
                            CompareToEmptyString {
                                lhs: unparse_expr(lhs, checker.stylist),
                                op: op.into(),
                                rhs: unparse_constant(value, checker.stylist),
                            },
                            Range::from_located(lhs)
                        );
                        checker.diagnostics.push(diag);
                    }
                }
            }
        }
    }
}
