use ruff_formatter::{format_args, write, Buffer, FormatResult};
use ruff_python_ast::MatchCase;

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::parentheses::parenthesized;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatMatchCase;

impl FormatNodeRule<MatchCase> for FormatMatchCase {
    fn fmt_fields(&self, item: &MatchCase, f: &mut PyFormatter) -> FormatResult<()> {
        let MatchCase {
            range: _,
            pattern,
            guard,
            body,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_item_comments = comments.dangling_comments(item);

        write!(f, [text("case"), space()])?;
        let leading_pattern_comments = comments.leading_comments(pattern);
        if leading_pattern_comments.is_empty() {
            pattern.format().fmt(f)?;
        } else {
            parenthesized(
                "(",
                &format_args![leading_comments(leading_pattern_comments), pattern.format()],
                ")",
            )
            .fmt(f)?;
        }

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
