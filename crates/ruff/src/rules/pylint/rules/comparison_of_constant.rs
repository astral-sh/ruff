use std::fmt;

use itertools::Itertools;
use rustpython_parser::ast::{self, Cmpop, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

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

/// ## What it does
/// Checks for comparisons between constants.
///
/// ## Why is this bad?
/// Comparing two constants will always resolve to the same value, so the
/// comparison is redundant. Instead, the expression should be replaced
/// with the result of the comparison.
///
/// ## Example
/// ```python
/// foo = 1 == 1
/// ```
///
/// Use instead:
/// ```python
/// foo = True
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/expressions.html#comparisons)
#[violation]
pub struct ComparisonOfConstant {
    left_constant: String,
    op: ViolationsCmpop,
    right_constant: String,
}

impl Violation for ComparisonOfConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ComparisonOfConstant {
            left_constant,
            op,
            right_constant,
        } = self;

        format!(
            "Two constants compared in a comparison, consider replacing `{left_constant} {op} \
             {right_constant}`"
        )
    }
}

/// PLR0133
pub(crate) fn comparison_of_constant(
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
        if let (
            Expr::Constant(ast::ExprConstant {
                value: left_constant,
                ..
            }),
            Expr::Constant(ast::ExprConstant {
                value: right_constant,
                ..
            }),
        ) = (&left, &right)
        {
            let diagnostic = Diagnostic::new(
                ComparisonOfConstant {
                    left_constant: checker.generator().constant(left_constant),
                    op: op.into(),
                    right_constant: checker.generator().constant(right_constant),
                },
                left.range(),
            );

            checker.diagnostics.push(diagnostic);
        };
    }
}
