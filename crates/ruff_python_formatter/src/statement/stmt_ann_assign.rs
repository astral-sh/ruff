use ruff_formatter::write;
use ruff_python_ast::StmtAnnAssign;

use crate::comments::{SourceComment, SuppressionKind};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtAnnAssign;

impl FormatNodeRule<StmtAnnAssign> for FormatStmtAnnAssign {
    fn fmt_fields(&self, item: &StmtAnnAssign, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAnnAssign {
            range: _,
            target,
            annotation,
            value,
            simple: _,
        } = item;

        write!(
            f,
            [
                target.format(),
                token(":"),
                space(),
                maybe_parenthesize_expression(annotation, item, Parenthesize::IfBreaks)
            ]
        )?;

        if let Some(value) = value {
            write!(
                f,
                [
                    space(),
                    token("="),
                    space(),
                    maybe_parenthesize_expression(value, item, Parenthesize::IfBreaks)
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
