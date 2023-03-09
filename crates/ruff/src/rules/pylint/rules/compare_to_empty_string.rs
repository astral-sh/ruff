use anyhow::anyhow;
use itertools::Itertools;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{unparse_constant, unparse_expr};
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind, Located};

use crate::{checkers::ast::Checker, registry::Diagnostic, violation::Violation};

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EmptyStringViolationsCmpop {
    Is,
    IsNot,
    Eq,
    NotEq,
}

impl TryFrom<&Cmpop> for EmptyStringViolationsCmpop {
    type Error = anyhow::Error;

    fn try_from(value: &Cmpop) -> Result<Self, Self::Error> {
        match value {
            Cmpop::Is => Ok(Self::Is),
            Cmpop::IsNot => Ok(Self::IsNot),
            Cmpop::Eq => Ok(Self::Eq),
            Cmpop::NotEq => Ok(Self::NotEq),
            _ => Err(anyhow!(
                "{value:?} cannot be converted to EmptyStringViolationsCmpop"
            )),
        }
    }
}

impl std::fmt::Display for EmptyStringViolationsCmpop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let repr = match self {
            Self::Is => "is",
            Self::IsNot => "is not",
            Self::Eq => "==",
            Self::NotEq => "!=",
        };
        write!(f, "{repr}")
    }
}

#[violation]
pub struct CompareToEmptyString {
    pub lhs: String,
    pub op: EmptyStringViolationsCmpop,
    pub rhs: String,
}

impl Violation for CompareToEmptyString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let prefix = match self.op {
            EmptyStringViolationsCmpop::Is | EmptyStringViolationsCmpop::Eq => "",
            EmptyStringViolationsCmpop::IsNot | EmptyStringViolationsCmpop::NotEq => "not ",
        };
        format!(
            "`{} {} {}` can be simplified to `{}{}` as an empty string is falsey",
            self.lhs, self.op, self.rhs, prefix, self.lhs
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
                                // we know `op` can be safely converted into a
                                // EmptyStringViolationCmpop due to the first if statement being
                                // true in this branch
                                op: op.try_into().unwrap(),
                                rhs: unparse_constant(value, checker.stylist),
                            },
                            lhs.into(),
                        );
                        checker.diagnostics.push(diag);
                    }
                }
            }
        }
    }
}
