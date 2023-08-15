use ruff_formatter::{format_args, write};
use ruff_python_ast::{Expr, Ranged, Stmt, StmtFor};

use crate::comments::{
    leading_alternate_branch_comments, trailing_comments, SourceComment, SuppressionKind,
};
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::verbatim::{OrElseParent, SuppressedClauseHeader};
use crate::FormatNodeRule;

#[derive(Debug)]
struct ExprTupleWithoutParentheses<'a>(&'a Expr);

impl Format<PyFormatContext<'_>> for ExprTupleWithoutParentheses<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        match self.0 {
            Expr::Tuple(expr_tuple) => expr_tuple
                .format()
                .with_options(TupleParentheses::NeverPreserve)
                .fmt(f),
            other => maybe_parenthesize_expression(other, self.0, Parenthesize::IfBreaks).fmt(f),
        }
    }
}

#[derive(Default)]
pub struct FormatStmtFor;

impl FormatNodeRule<StmtFor> for FormatStmtFor {
    fn fmt_fields(&self, item: &StmtFor, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtFor {
            is_async,
            target,
            iter,
            body,
            orelse,
            range: _,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling_comments(item);
        let body_start = body.first().map_or(iter.end(), Stmt::start);
        let or_else_comments_start =
            dangling_comments.partition_point(|comment| comment.slice().end() < body_start);

        let (trailing_condition_comments, or_else_comments) =
            dangling_comments.split_at(or_else_comments_start);

        if SuppressionKind::has_skip_comment(trailing_condition_comments, f.context().source()) {
            SuppressedClauseHeader::For(item).fmt(f)?;
        } else {
            write!(
                f,
                [
                    is_async.then_some(format_args![text("async"), space()]),
                    text("for"),
                    space(),
                    ExprTupleWithoutParentheses(target),
                    space(),
                    text("in"),
                    space(),
                    maybe_parenthesize_expression(iter, item, Parenthesize::IfBreaks),
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

        if orelse.is_empty() {
            debug_assert!(or_else_comments.is_empty());
        } else {
            // Split between leading comments before the `else` keyword and end of line comments at the end of
            // the `else:` line.
            let trailing_start =
                or_else_comments.partition_point(|comment| comment.line_position().is_own_line());
            let (leading, trailing) = or_else_comments.split_at(trailing_start);

            leading_alternate_branch_comments(leading, body.last()).fmt(f)?;

            if SuppressionKind::has_skip_comment(trailing_condition_comments, f.context().source())
            {
                SuppressedClauseHeader::OrElse(OrElseParent::For(item)).fmt(f)?;
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
