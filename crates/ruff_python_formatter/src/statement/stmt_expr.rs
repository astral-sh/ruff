use ruff_python_ast::{self as ast, Expr, Operator, StmtExpr};

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::statement::trailing_semicolon;

#[derive(Default)]
pub struct FormatStmtExpr;

impl FormatNodeRule<ruff_python_ast::kind::StmtExpr> for FormatStmtExpr {
    fn fmt_fields(&self, item: StmtExpr<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let value = item.value();

        if is_arithmetic_like(value) {
            maybe_parenthesize_expression(value, item, Parenthesize::Optional).fmt(f)?;
        } else {
            value.format().fmt(f)?;
        }

        if f.options().source_type().is_ipynb()
            && f.context().node_level().is_last_top_level_statement()
            && trailing_semicolon(item.into(), f.context().source()).is_some()
        {
            token(";").fmt(f)?;
        }

        Ok(())
    }
}

fn is_arithmetic_like(expression: Expr<'_>) -> bool {
    matches!(
        expression.fields(),
        ast::ExprFields::BinOp(ast::ExprBinOpFields {
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
