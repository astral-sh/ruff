use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::StmtMatch;

use crate::comments::dangling_comments;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatStmtMatch;

impl FormatNodeRule<StmtMatch> for FormatStmtMatch {
    fn fmt_fields(&self, item: &StmtMatch, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtMatch {
            range: _,
            subject,
            cases,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_item_comments = comments.dangling_comments(item);

        // There can be at most one dangling comment after the colon in a match statement.
        debug_assert!(dangling_item_comments.len() <= 1);

        write!(
            f,
            [
                text("match"),
                space(),
                maybe_parenthesize_expression(subject, item, Parenthesize::IfBreaks),
                text(":"),
                dangling_comments(dangling_item_comments)
            ]
        )?;

        for case in cases {
            write!(f, [block_indent(&case.format())])?;
        }

        Ok(())
    }

    fn fmt_dangling_comments(&self, _node: &StmtMatch, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled as part of `fmt_fields`
        Ok(())
    }
}
