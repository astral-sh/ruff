use ruff_formatter::write;
use ruff_python_ast::StmtAugAssign;

use crate::comments::{SourceComment, SuppressionKind};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::{AsFormat, FormatNodeRule};

#[derive(Default)]
pub struct FormatStmtAugAssign;

impl FormatNodeRule<StmtAugAssign> for FormatStmtAugAssign {
    fn fmt_fields(&self, item: &StmtAugAssign, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAugAssign {
            target,
            op,
            value,
            range: _,
        } = item;
        write!(
            f,
            [
                target.format(),
                space(),
                op.format(),
                token("="),
                space(),
                maybe_parenthesize_expression(value, item, Parenthesize::IfBreaks)
            ]
        )
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        SuppressionKind::has_skip_comment(trailing_comments, context.source())
    }
}
