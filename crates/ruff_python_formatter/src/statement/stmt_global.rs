use ruff_formatter::{format_args, write};
use ruff_python_ast::AstNode;
use ruff_python_ast::StmtGlobal;

use crate::comments::{SourceComment, SuppressionKind};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtGlobal;

impl FormatNodeRule<StmtGlobal> for FormatStmtGlobal {
    fn fmt_fields(&self, item: &StmtGlobal, f: &mut PyFormatter) -> FormatResult<()> {
        // Join the `global` names, breaking across continuation lines if necessary, unless the
        // `global` statement has a trailing comment, in which case, breaking the names would
        // move the comment "off" of the `global` statement.
        if f.context().comments().has_trailing(item.as_any_node_ref()) {
            let joined = format_with(|f| {
                f.join_with(format_args![token(","), space()])
                    .entries(item.names.iter().formatted())
                    .finish()
            });

            write!(f, [token("global"), space(), &joined])
        } else {
            let joined = format_with(|f| {
                f.join_with(&format_args![
                    token(","),
                    space(),
                    if_group_breaks(&token("\\")),
                    soft_line_break(),
                ])
                .entries(item.names.iter().formatted())
                .finish()
            });

            write!(
                f,
                [
                    token("global"),
                    space(),
                    group(&format_args!(
                        if_group_breaks(&token("\\")),
                        soft_line_break(),
                        soft_block_indent(&joined)
                    ))
                ]
            )
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
