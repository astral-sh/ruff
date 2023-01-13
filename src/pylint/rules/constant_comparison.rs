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
                value: left_value, ..
            },
            ExprKind::Constant {
                value: right_value, ..
            },
        ) = (&left.node, &right.node)
        {
            let diagnostic = Diagnostic::new(
                violations::ConstantComparison(
                    left_value.to_string(),
                    right_value.to_string(),
                    op.into(),
                ),
                Range::from_located(left),
            );

            checker.diagnostics.push(diagnostic);
        };
    }
}
