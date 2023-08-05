use ruff_python_ast as ast;
use ruff_python_ast::{Expr, Operator, StmtExpr};

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtExpr;

impl FormatNodeRule<StmtExpr> for FormatStmtExpr {
    fn fmt_fields(&self, item: &StmtExpr, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtExpr { value, .. } = item;

        if is_arithmetic_like(value) {
            maybe_parenthesize_expression(value, item, Parenthesize::Optional).fmt(f)
        } else {
            value.format().fmt(f)
        }
    }
}

const fn is_arithmetic_like(expression: &Expr) -> bool {
    matches!(
        expression,
        Expr::BinOp(ast::ExprBinOp {
            op: Operator::BitOr
                | Operator::BitXor
                | Operator::LShift
                | Operator::RShift
                | Operator::Add
                | Operator::Sub,
            ..
        })
    )
}
