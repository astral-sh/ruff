use ruff_formatter::write;
use ruff_python_ast::StmtReturn;

use crate::comments::{SourceComment, SuppressionKind};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtReturn;

impl FormatNodeRule<StmtReturn> for FormatStmtReturn {
    fn fmt_fields(&self, item: &StmtReturn, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtReturn { range: _, value } = item;
        if let Some(value) = value {
            write!(
                f,
                [
                    text("return"),
                    space(),
                    maybe_parenthesize_expression(value, item, Parenthesize::IfBreaks)
                ]
            )
        } else {
            text("return").fmt(f)
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
