use ruff_formatter::{format_args, write};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{ElifElseClause, StmtIf};

use crate::comments::SourceComment;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::statement::clause::{clause_header, ClauseHeader};
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtIf;

impl FormatNodeRule<StmtIf> for FormatStmtIf {
    fn fmt_fields(&self, item: &StmtIf, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtIf {
            range: _,
            test,
            body,
            elif_else_clauses,
        } = item;

        let comments = f.context().comments().clone();
        let trailing_colon_comment = comments.dangling_comments(item);

        write!(
            f,
            [
                clause_header(
                    ClauseHeader::If(item),
                    trailing_colon_comment,
                    &format_args![
                        text("if"),
                        space(),
                        maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks),
                    ],
                ),
                block_indent(&body.format())
            ]
        )?;

        let mut last_node = body.last().unwrap().into();
        for clause in elif_else_clauses {
            format_elif_else_clause(clause, f, Some(last_node))?;
            last_node = clause.body.last().unwrap().into();
        }

        Ok(())
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled by `fmt_fields`
        Ok(())
    }
}

/// Extracted so we can implement `FormatElifElseClause` but also pass in `last_node` from
/// `FormatStmtIf`
pub(crate) fn format_elif_else_clause(
    item: &ElifElseClause,
    f: &mut PyFormatter,
    last_node: Option<AnyNodeRef>,
) -> FormatResult<()> {
    let ElifElseClause {
        range: _,
        test,
        body,
    } = item;

    let comments = f.context().comments().clone();
    let trailing_colon_comment = comments.dangling_comments(item);
    let leading_comments = comments.leading_comments(item);

    write!(
        f,
        [
            clause_header(
                ClauseHeader::ElifElse(item),
                trailing_colon_comment,
                &format_with(|f| {
                    if let Some(test) = test {
                        write!(
                            f,
                            [
                                text("elif"),
                                space(),
                                maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks),
                            ]
                        )
                    } else {
                        text("else").fmt(f)
                    }
                }),
            )
            .with_leading_comments(leading_comments, last_node),
            block_indent(&body.format())
        ]
    )
}
