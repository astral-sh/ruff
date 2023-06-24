use crate::comments::trailing_comments;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{format_args, write, Buffer, FormatResult};
use ruff_python_ast::prelude::*;
use rustpython_parser::ast::StmtWith;

#[derive(Default)]
pub struct FormatStmtWith;

impl FormatNodeRule<StmtWith> for FormatStmtWith {
    fn fmt_fields(&self, item: &StmtWith, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtWith {
            range: _,
            items,
            body,
            type_comment: _,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling_comments(item.as_any_node_ref());

        let joined_items = format_with(|f| f.join_comma_separated().nodes(items.iter()).finish());

        write!(
            f,
            [
                text("with"),
                space(),
                group(&format_args![
                    if_group_breaks(&text("(")),
                    soft_block_indent(&joined_items),
                    if_group_breaks(&text(")")),
                ]),
                text(":"),
                trailing_comments(dangling_comments),
                block_indent(&body.format())
            ]
        )
    }

    fn fmt_dangling_comments(&self, _node: &StmtWith, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
