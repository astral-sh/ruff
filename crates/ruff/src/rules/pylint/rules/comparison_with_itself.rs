use std::fmt;

use itertools::Itertools;
use rustpython_parser::ast::{Cmpop, Expr};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum ViolationsCmpop {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn,
}

impl From<&Cmpop> for ViolationsCmpop {
    fn from(cmpop: &Cmpop) -> Self {
        match cmpop {
            Cmpop::Eq => Self::Eq,
            Cmpop::NotEq => Self::NotEq,
            Cmpop::Lt => Self::Lt,
            Cmpop::LtE => Self::LtE,
            Cmpop::Gt => Self::Gt,
            Cmpop::GtE => Self::GtE,
            Cmpop::Is => Self::Is,
            Cmpop::IsNot => Self::IsNot,
            Cmpop::In => Self::In,
            Cmpop::NotIn => Self::NotIn,
        }
    }
}

impl fmt::Display for ViolationsCmpop {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let representation = match self {
            Self::Eq => "==",
            Self::NotEq => "!=",
            Self::Lt => "<",
            Self::LtE => "<=",
            Self::Gt => ">",
            Self::GtE => ">=",
            Self::Is => "is",
            Self::IsNot => "is not",
            Self::In => "in",
            Self::NotIn => "not in",
        };
        write!(f, "{representation}")
    }
}

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
