use ruff_python_ast as ast;
use ruff_python_ast::{Expr, Operator, StmtExpr};

use crate::comments::{SourceComment, SuppressionKind};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;

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

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        SuppressionKind::has_skip_comment(trailing_comments, context.source())
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
