use ruff_python_ast::StmtBreak;

use crate::comments::{SourceComment, SuppressionKind};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtBreak;

impl FormatNodeRule<StmtBreak> for FormatStmtBreak {
    fn fmt_fields(&self, _item: &StmtBreak, f: &mut PyFormatter) -> FormatResult<()> {
        token("break").fmt(f)
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        SuppressionKind::has_skip_comment(trailing_comments, context.source())
    }
}
