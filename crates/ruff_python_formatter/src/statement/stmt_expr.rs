use ruff_python_ast as ast;
use ruff_python_ast::{Expr, Operator, StmtExpr};

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{Parentheses, Parenthesize};
use crate::prelude::*;
use crate::preview::{
    is_parenthesize_tuple_in_yield_enabled, is_remove_redundant_top_level_parentheses_enabled,
};
use crate::statement::trailing_semicolon;

#[derive(Default)]
pub struct FormatStmtExpr;

impl FormatNodeRule<StmtExpr> for FormatStmtExpr {
    fn fmt_fields(&self, item: &StmtExpr, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtExpr { value, .. } = item;

        if is_remove_redundant_top_level_parentheses_enabled(f.context())
            && f.context()
                .is_expression_parenthesized(value.as_ref().into())
        {
            if matches!(
                value.as_ref(),
                Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Lambda(_)
            ) {
                // These statements can wrap their values or bodies independently,
                // but comments in their headers can require an outer pair.
                let parentheses = if f
                    .context()
                    .comments()
                    .contains_comments(value.as_ref().into())
                {
                    Parentheses::Preserve
                } else {
                    Parentheses::Never
                };
                value.format().with_options(parentheses).fmt(f)?;
            } else {
                let parenthesize = if matches!(
                    value.as_ref(),
                    Expr::Name(_)
                        | Expr::NumberLiteral(_)
                        | Expr::BooleanLiteral(_)
                        | Expr::NoneLiteral(_)
                        | Expr::EllipsisLiteral(_)
                ) {
                    Parenthesize::IfRequired
                } else {
                    Parenthesize::IfBreaks
                };
                maybe_parenthesize_expression(value, item, parenthesize).fmt(f)?;
            }
        } else if is_parenthesize_tuple_in_yield_enabled(f.context())
            && let Expr::Yield(yield_expr) = value.as_ref()
            && yield_expr
                .value
                .as_ref()
                .is_some_and(|value| value.is_tuple_expr())
            && !f.context().comments().contains_comments(yield_expr.into())
        {
            value.format().with_options(Parentheses::Never).fmt(f)?;
        } else if is_arithmetic_like(value) {
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
