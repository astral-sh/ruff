use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::{StmtWith, WithItem};

use crate::builders::optional_parentheses;
use crate::comments::trailing_comments;
use crate::prelude::*;
use crate::statement::Stmt;
use crate::{FormatNodeRule, PyFormatter};

pub(super) trait FormatWithLike<'ast>: ruff_python_ast::node::AstNode {
    // whether this is `with` or `async with`
    const ASYNC: bool;

    // extract the items and the body
    fn destruct(&self) -> (&Vec<WithItem>, &Vec<Stmt>);

    fn fmt_with(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let (items, body) = self.destruct();

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling_comments(self.as_any_node_ref());

        let joined_items = format_with(|f| f.join_comma_separated().nodes(items.iter()).finish());

        if Self::ASYNC {
            write!(f, [text("async"), space()])?;
        }

        write!(
            f,
            [
                text("with"),
                space(),
                group(&optional_parentheses(&joined_items)),
                text(":"),
                trailing_comments(dangling_comments),
                block_indent(&body.format())
            ]
        )
    }
}

#[derive(Default)]
pub struct FormatStmtWith;

impl<'ast> FormatWithLike<'ast> for StmtWith {
    const ASYNC: bool = false;

    fn destruct(&self) -> (&Vec<WithItem>, &Vec<Stmt>) {
        (&self.items, &self.body)
    }
}

impl FormatNodeRule<StmtWith> for FormatStmtWith {
    fn fmt_fields(&self, item: &StmtWith, f: &mut PyFormatter) -> FormatResult<()> {
        item.fmt_with(f)
    }

    fn fmt_dangling_comments(&self, _node: &StmtWith, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
