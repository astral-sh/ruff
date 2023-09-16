use ruff_formatter::write;
use ruff_python_ast::{Expr, StmtReturn};

use crate::comments::{SourceComment, SuppressionKind};
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtReturn;

impl FormatNodeRule<StmtReturn> for FormatStmtReturn {
    fn fmt_fields(&self, item: &StmtReturn, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtReturn { range: _, value } = item;

        token("return").fmt(f)?;

        match value.as_deref() {
            Some(Expr::Tuple(tuple)) if !f.context().comments().has_leading(tuple) => {
                write!(
                    f,
                    [
                        space(),
                        tuple
                            .format()
                            .with_options(TupleParentheses::OptionalParentheses)
                    ]
                )
            }
            Some(value) => {
                write!(
                    f,
                    [
                        space(),
                        maybe_parenthesize_expression(value, item, Parenthesize::IfBreaks)
                    ]
                )
            }
            None => Ok(()),
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
