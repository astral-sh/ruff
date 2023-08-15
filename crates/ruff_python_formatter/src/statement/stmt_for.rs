use ruff_formatter::{format_args, write, Buffer, FormatResult};
use ruff_python_ast::{Expr, Ranged, Stmt, StmtFor};

use crate::comments::{leading_alternate_branch_comments, trailing_comments, SourceComment};
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};

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

        write!(
            f,
            [
                is_async.then_some(format_args![text("async"), space()]),
                text("for"),
                space(),
                ExprWithTupleParentheses {
                    expr: target,
                    parent: item,
                    parentheses: TupleParentheses::NeverPreserve
                },
                space(),
                text("in"),
                space(),
                ExprWithTupleParentheses {
                    expr: iter,
                    parent: item,
                    parentheses: TupleParentheses::Preserve
                },
                text(":"),
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

            write!(
                f,
                [
                    leading_alternate_branch_comments(leading, body.last()),
                    text("else:"),
                    trailing_comments(trailing),
                    block_indent(&orelse.format())
                ]
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

#[derive(Debug)]
struct ExprWithTupleParentheses<'a> {
    expr: &'a Expr,
    parent: &'a StmtFor,
    parentheses: TupleParentheses,
}

impl Format<PyFormatContext<'_>> for ExprWithTupleParentheses<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        match &self.expr {
            Expr::Tuple(expr_tuple) => expr_tuple.format().with_options(self.parentheses).fmt(f),
            other => {
                maybe_parenthesize_expression(other, self.parent, Parenthesize::IfBreaks).fmt(f)
            }
        }
    }
}
