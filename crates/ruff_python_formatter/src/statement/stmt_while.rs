use ruff_formatter::write;
use ruff_python_ast::node::AstNode;
use ruff_python_ast::{Ranged, Stmt, StmtWhile};

use crate::comments::{
    leading_alternate_branch_comments, trailing_comments, SourceComment, SuppressionKind,
};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::verbatim::{OrElseParent, SuppressedClauseHeader};
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtWhile;

impl FormatNodeRule<StmtWhile> for FormatStmtWhile {
    fn fmt_fields(&self, item: &StmtWhile, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtWhile {
            range: _,
            test,
            body,
            orelse,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling_comments(item.as_any_node_ref());

        let body_start = body.first().map_or(test.end(), Stmt::start);
        let or_else_comments_start =
            dangling_comments.partition_point(|comment| comment.slice().end() < body_start);

        let (trailing_condition_comments, or_else_comments) =
            dangling_comments.split_at(or_else_comments_start);

        if SuppressionKind::has_skip_comment(trailing_condition_comments, f.context().source()) {
            SuppressedClauseHeader::While(item).fmt(f)?;
        } else {
            write!(
                f,
                [
                    text("while"),
                    space(),
                    maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks),
                    text(":"),
                ]
            )?;
        }

        write!(
            f,
            [
                trailing_comments(trailing_condition_comments),
                block_indent(&body.format())
            ]
        )?;

        if !orelse.is_empty() {
            // Split between leading comments before the `else` keyword and end of line comments at the end of
            // the `else:` line.
            let trailing_start =
                or_else_comments.partition_point(|comment| comment.line_position().is_own_line());
            let (leading, trailing) = or_else_comments.split_at(trailing_start);

            leading_alternate_branch_comments(leading, body.last()).fmt(f)?;

            if SuppressionKind::has_skip_comment(trailing, f.context().source()) {
                SuppressedClauseHeader::OrElse(OrElseParent::While(item)).fmt(f)?;
            } else {
                text("else:").fmt(f)?;
            }

            write!(
                f,
                [trailing_comments(trailing), block_indent(&orelse.format())]
            )?;
        }

        Ok(())
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
