use ruff_formatter::write;
use ruff_python_ast::{Expr, StmtReturn};

use crate::comments::SourceComment;
use crate::expression::expr_tuple::TupleParentheses;
use crate::statement::stmt_assign::FormatStatementsLastExpression;
use crate::{has_skip_comment, prelude::*};

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
                        FormatStatementsLastExpression::left_to_right(value, item)
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
        has_skip_comment(trailing_comments, context.source())
    }
}
