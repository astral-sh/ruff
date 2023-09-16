use ruff_python_ast::StmtIpyEscapeCommand;
use ruff_text_size::Ranged;

use crate::comments::{SourceComment, SuppressionKind};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtIpyEscapeCommand;

impl FormatNodeRule<StmtIpyEscapeCommand> for FormatStmtIpyEscapeCommand {
    fn fmt_fields(&self, item: &StmtIpyEscapeCommand, f: &mut PyFormatter) -> FormatResult<()> {
        source_text_slice(item.range()).fmt(f)
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        SuppressionKind::has_skip_comment(trailing_comments, context.source())
    }
}
