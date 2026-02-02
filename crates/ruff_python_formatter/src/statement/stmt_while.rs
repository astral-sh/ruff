use ruff_formatter::{format_args, write};
use ruff_python_ast::{Stmt, StmtWhile};
use ruff_text_size::Ranged;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::statement::clause::{ClauseHeader, ElseClause, clause};
use crate::statement::suite::SuiteKind;

#[derive(Default)]
pub struct FormatStmtWhile;

impl FormatNodeRule<StmtWhile> for FormatStmtWhile {
    fn fmt_fields(&self, item: &StmtWhile, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtWhile {
            range: _,
            node_index: _,
            test,
            body,
            orelse,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling(item);

        let body_start = body.first().map_or(test.end(), Stmt::start);
        let or_else_comments_start =
            dangling_comments.partition_point(|comment| comment.end() < body_start);

        let (trailing_condition_comments, or_else_comments) =
            dangling_comments.split_at(or_else_comments_start);

        write!(
            f,
            [clause(
                ClauseHeader::While(item),
                &format_args![
                    token("while"),
                    space(),
                    maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks),
                ],
                trailing_condition_comments,
                body,
                SuiteKind::other(orelse.is_empty()),
            )]
        )?;

        if !orelse.is_empty() {
            // Split between leading comments before the `else` keyword and end of line comments at the end of
            // the `else:` line.
            let trailing_start =
                or_else_comments.partition_point(|comment| comment.line_position().is_own_line());
            let (leading, trailing) = or_else_comments.split_at(trailing_start);

            write!(
                f,
                [clause(
                    ClauseHeader::OrElse(ElseClause::While(item)),
                    &token("else"),
                    trailing,
                    orelse,
                    SuiteKind::other(true),
                )
                .with_leading_comments(leading, body.last()),]
            )?;
        }

        Ok(())
    }
}
