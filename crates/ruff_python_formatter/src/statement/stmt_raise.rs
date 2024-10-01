use ruff_formatter::write;
use ruff_python_ast::StmtRaise;

use crate::comments::SourceComment;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::{has_skip_comment, prelude::*};

#[derive(Default)]
pub struct FormatStmtRaise;

impl FormatNodeRule<StmtRaise> for FormatStmtRaise {
    fn fmt_fields(&self, item: &StmtRaise, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtRaise {
            range: _,
            exc,
            cause,
        } = item;

        token("raise").fmt(f)?;

        if let Some(value) = exc {
            write!(
                f,
                [
                    space(),
                    maybe_parenthesize_expression(value, item, Parenthesize::Optional)
                ]
            )?;
        }

        if let Some(value) = cause {
            write!(
                f,
                [
                    space(),
                    token("from"),
                    space(),
                    maybe_parenthesize_expression(value, item, Parenthesize::Optional)
                ]
            )?;
        }
        Ok(())
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        has_skip_comment(trailing_comments, context.source())
    }
}
