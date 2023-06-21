use crate::comments::{leading_alternate_branch_comments, trailing_comments, SourceComment};
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::{write, FormatError};
use rustpython_parser::ast::{Ranged, Stmt, StmtIf, Suite};

#[derive(Default)]
pub struct FormatStmtIf;

impl FormatNodeRule<StmtIf> for FormatStmtIf {
    fn fmt_fields(&self, item: &StmtIf, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        let mut current = IfOrElIf::If(item);
        let mut else_comments: &[SourceComment];
        let mut last_node_of_previous_body = None;

        loop {
            let current_statement = current.statement();
            let StmtIf {
                test, body, orelse, ..
            } = current_statement;

            let first_statement = body.first().ok_or(FormatError::SyntaxError)?;
            let trailing = comments.dangling_comments(current_statement);

            let trailing_if_comments_end = trailing
                .partition_point(|comment| comment.slice().start() < first_statement.start());

            let (if_trailing_comments, trailing_alternate_comments) =
                trailing.split_at(trailing_if_comments_end);

            if current.is_elif() {
                let elif_leading = comments.leading_comments(current_statement);
                // Manually format the leading comments because the formatting bypasses `NodeRule::fmt`
                write!(
                    f,
                    [
                        leading_alternate_branch_comments(elif_leading, last_node_of_previous_body),
                        source_position(current_statement.start())
                    ]
                )?;
            }

            write!(
                f,
                [
                    text(current.keyword()),
                    space(),
                    test.format().with_options(Parenthesize::IfBreaks),
                    text(":"),
                    trailing_comments(if_trailing_comments),
                    block_indent(&body.format())
                ]
            )?;

            // RustPython models `elif` by setting the body to a single `if` statement. The `orelse`
            // of the most inner `if` statement then becomes the `else` of the whole `if` chain.
            // That's why it's necessary to take the comments here from the most inner `elif`.
            else_comments = trailing_alternate_comments;
            last_node_of_previous_body = body.last();

            if let Some(elif) = else_if(orelse) {
                current = elif;
            } else {
                break;
            }
        }

        let orelse = &current.statement().orelse;

        if !orelse.is_empty() {
            // Leading comments are always own line comments
            let leading_else_comments_end =
                else_comments.partition_point(|comment| comment.line_position().is_own_line());
            let (else_leading, else_trailing) = else_comments.split_at(leading_else_comments_end);

            write!(
                f,
                [
                    leading_alternate_branch_comments(else_leading, last_node_of_previous_body),
                    text("else:"),
                    trailing_comments(else_trailing),
                    block_indent(&orelse.format())
                ]
            )?;
        }

        Ok(())
    }

    fn fmt_dangling_comments(&self, _node: &StmtIf, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled by `fmt_fields`
        Ok(())
    }
}

fn else_if(or_else: &Suite) -> Option<IfOrElIf> {
    if let [Stmt::If(if_stmt)] = or_else.as_slice() {
        Some(IfOrElIf::ElIf(if_stmt))
    } else {
        None
    }
}

enum IfOrElIf<'a> {
    If(&'a StmtIf),
    ElIf(&'a StmtIf),
}

impl<'a> IfOrElIf<'a> {
    const fn statement(&self) -> &'a StmtIf {
        match self {
            IfOrElIf::If(statement) => statement,
            IfOrElIf::ElIf(statement) => statement,
        }
    }

    const fn keyword(&self) -> &'static str {
        match self {
            IfOrElIf::If(_) => "if",
            IfOrElIf::ElIf(_) => "elif",
        }
    }

    const fn is_elif(&self) -> bool {
        matches!(self, IfOrElIf::ElIf(_))
    }
}
