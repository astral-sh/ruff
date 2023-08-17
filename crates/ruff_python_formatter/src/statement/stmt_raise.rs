use crate::comments::{SourceComment, SuppressionKind};
use ruff_formatter::write;
use ruff_python_ast::StmtRaise;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtRaise;

impl FormatNodeRule<StmtRaise> for FormatStmtRaise {
    fn fmt_fields(&self, item: &StmtRaise, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtRaise {
            range: _,
            exc,
            cause,
        } = item;

        text("raise").fmt(f)?;

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
                    text("from"),
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
        SuppressionKind::has_skip_comment(trailing_comments, context.source())
    }
}
