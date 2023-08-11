use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::MatchCase;

use crate::comments::trailing_comments;
use crate::not_yet_implemented_custom_text;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatMatchCase;

impl FormatNodeRule<MatchCase> for FormatMatchCase {
    fn fmt_fields(&self, item: &MatchCase, f: &mut PyFormatter) -> FormatResult<()> {
        let MatchCase {
            range: _,
            pattern: _,
            guard,
            body,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_item_comments = comments.dangling_comments(item);

        write!(
            f,
            [
                text("case"),
                space(),
                not_yet_implemented_custom_text("NOT_YET_IMPLEMENTED_Pattern"),
            ]
        )?;

        if let Some(guard) = guard {
            write!(f, [space(), text("if"), space(), guard.format()])?;
        }

        write!(
            f,
            [
                text(":"),
                trailing_comments(dangling_item_comments),
                block_indent(&body.format())
            ]
        )
    }

    fn fmt_dangling_comments(&self, _node: &MatchCase, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled as part of `fmt_fields`
        Ok(())
    }
}
