use ruff_formatter::{format_args, write};
use ruff_python_ast::StmtNonlocal;

use crate::comments::SourceComment;
use crate::has_skip_comment;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtNonlocal;

impl FormatNodeRule<StmtNonlocal> for FormatStmtNonlocal {
    fn fmt_fields(&self, item: &StmtNonlocal, f: &mut PyFormatter) -> FormatResult<()> {
        // Join the `nonlocal` names, breaking across continuation lines if necessary, unless the
        // `nonlocal` statement has a trailing comment, in which case, breaking the names would
        // move the comment "off" of the `nonlocal` statement.
        if f.context().comments().has_trailing(item) {
            let joined = format_with(|f| {
                f.join_with(format_args![token(","), space()])
                    .entries(item.names.iter().formatted())
                    .finish()
            });

            write!(f, [token("nonlocal"), space(), &joined])
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
                    token("nonlocal"),
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
        has_skip_comment(trailing_comments, context.source())
    }
}
