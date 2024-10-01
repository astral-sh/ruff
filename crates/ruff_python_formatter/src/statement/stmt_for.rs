use ruff_formatter::{format_args, write};
use ruff_python_ast::{Expr, Stmt, StmtFor};
use ruff_text_size::Ranged;

use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::statement::clause::{clause_body, clause_header, ClauseHeader, ElseClause};
use crate::statement::suite::SuiteKind;

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
        let dangling_comments = comments.dangling(item);
        let body_start = body.first().map_or(iter.end(), Stmt::start);
        let or_else_comments_start =
            dangling_comments.partition_point(|comment| comment.end() < body_start);

        let (trailing_condition_comments, or_else_comments) =
            dangling_comments.split_at(or_else_comments_start);

        write!(
            f,
            [
                clause_header(
                    ClauseHeader::For(item),
                    trailing_condition_comments,
                    &format_args![
                        is_async.then_some(format_args![token("async"), space()]),
                        token("for"),
                        space(),
                        ExprTupleWithoutParentheses(target),
                        space(),
                        token("in"),
                        space(),
                        maybe_parenthesize_expression(iter, item, Parenthesize::IfBreaks),
                    ],
                ),
                clause_body(
                    body,
                    SuiteKind::other(orelse.is_empty()),
                    trailing_condition_comments
                ),
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
                    clause_header(
                        ClauseHeader::OrElse(ElseClause::For(item)),
                        trailing,
                        &token("else"),
                    )
                    .with_leading_comments(leading, body.last()),
                    clause_body(orelse, SuiteKind::other(true), trailing),
                ]
            )?;
        }

        Ok(())
    }
}
