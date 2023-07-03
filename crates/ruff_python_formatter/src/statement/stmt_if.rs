use crate::comments::{leading_alternate_branch_comments, trailing_comments};
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::{ElifElseClause, StmtIf};

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
                test.format().with_options(Parenthesize::IfBreaks),
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

/// Note that this implementation misses the leading newlines before the leading comments because
/// it does not have access to the last node of the previous branch. The `StmtIf` therefore doesn't
/// call this but `format_elif_else_clause` directly.
#[derive(Default)]
pub struct FormatElifElseClause;

impl FormatNodeRule<ElifElseClause> for FormatElifElseClause {
    fn fmt_fields(&self, item: &ElifElseClause, f: &mut PyFormatter) -> FormatResult<()> {
        format_elif_else_clause(item, f, None)
    }
}

/// Extracted so we can implement `FormatElifElseClause` but also pass in `last_node` from
/// `FormatStmtIf`
fn format_elif_else_clause(
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
                test.format().with_options(Parenthesize::IfBreaks),
                text(":")
            ]
        )?;
    } else {
        write!(f, [text("else"), text(":")])?;
    }

    write!(
        f,
        [
            trailing_comments(trailing_colon_comment),
            block_indent(&body.format())
        ]
    )
}
