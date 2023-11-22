use ruff_python_ast::StmtPass;

use crate::comments::{SourceComment, SuppressionKind};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtPass;

impl FormatNodeRule<StmtPass> for FormatStmtPass {
    fn fmt_fields(&self, _item: &StmtPass, f: &mut PyFormatter) -> FormatResult<()> {
        token("pass").fmt(f)
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        SuppressionKind::has_skip_comment(trailing_comments, context.source())
    }
}
