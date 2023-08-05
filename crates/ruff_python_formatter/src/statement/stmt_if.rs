use crate::comments::{leading_alternate_branch_comments, trailing_comments};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{ElifElseClause, StmtIf};

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
                text("if"),
                space(),
                maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks),
                text(":"),
                trailing_comments(trailing_colon_comment),
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

    fn fmt_dangling_comments(&self, _node: &StmtIf, _f: &mut PyFormatter) -> FormatResult<()> {
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

    leading_alternate_branch_comments(leading_comments, last_node).fmt(f)?;

    if let Some(test) = test {
        write!(
            f,
            [
                text("elif"),
                space(),
                maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks),
            ]
        )?;
    } else {
        text("else").fmt(f)?;
    }

    write!(
        f,
        [
            text(":"),
            trailing_comments(trailing_colon_comment),
            block_indent(&body.format())
        ]
    )
}
