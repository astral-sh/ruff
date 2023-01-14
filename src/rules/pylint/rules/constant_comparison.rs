use itertools::Itertools;
use rustpython_ast::{Cmpop, Expr, ExprKind, Located};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// PLR0133
pub fn constant_comparison(
    checker: &mut Checker,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    for ((left, right), op) in std::iter::once(left)
        .chain(comparators.iter())
        .tuple_windows::<(&Located<_>, &Located<_>)>()
        .zip(ops)
    {
        if let (
            ExprKind::Constant {
                value: left_constant,
                ..
            },
            ExprKind::Constant {
                value: right_constant,
                ..
            },
        ) = (&left.node, &right.node)
        {
            let diagnostic = Diagnostic::new(
                violations::ConstantComparison {
                    left_constant: left_constant.to_string(),
                    op: op.into(),
                    right_constant: right_constant.to_string(),
                },
                Range::from_located(left),
            );

            checker.diagnostics.push(diagnostic);
        };
    }
}
