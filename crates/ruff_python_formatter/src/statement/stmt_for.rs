use crate::comments::{leading_alternate_branch_comments, trailing_comments};
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AstNode;
use rustpython_parser::ast::{Ranged, Stmt, StmtFor};

#[derive(Default)]
pub struct FormatStmtFor;

impl FormatNodeRule<StmtFor> for FormatStmtFor {
    fn fmt_fields(&self, item: &StmtFor, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtFor {
            range: _,
            target,
            iter,
            body,
            orelse,
            type_comment: _,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling_comments(item.as_any_node_ref());
        let body_start = body.first().map_or(iter.end(), Stmt::start);
        let or_else_comments_start =
            dangling_comments.partition_point(|comment| comment.slice().end() < body_start);

        let (trailing_condition_comments, or_else_comments) =
            dangling_comments.split_at(or_else_comments_start);

        write!(
            f,
            [
                text("for"),
                space(),
                // TODO: the `IfBreaks` is currently ignored by
                // https://github.com/astral-sh/ruff/blob/4b9b6829dccabdd4faf6efa6a118b4868347a701/crates/ruff_python_formatter/src/expression/expr_tuple.rs#L78
                target.format().with_options(Parenthesize::IfBreaks),
                space(),
                text("in"),
                space(),
                iter.format().with_options(Parenthesize::IfBreaks),
                text(":"),
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

    fn fmt_dangling_comments(&self, _node: &StmtFor, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
