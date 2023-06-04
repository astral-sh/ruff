use crate::comments::dangling_comments;
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::{format_args, write};
use rustpython_parser::ast::ExprList;

#[derive(Default)]
pub struct FormatExprList;

impl FormatNodeRule<ExprList> for FormatExprList {
    fn fmt_fields(&self, item: &ExprList, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprList {
            range: _,
            elts,
            ctx: _,
        } = item;

        let items = format_with(|f| {
            let mut first = true;
            for item in elts {
                if !first {
                    write!(f, [text(","), soft_line_break_or_space()])?;
                }

                write!(f, [item.format()])?;

                first = false;
            }

            if !elts.is_empty() {
                write!(f, [if_group_breaks(&text(","))])?;
            }

            Ok(())
        });

        let comments = f.context().comments().clone();
        let dangling = comments.dangling_comments(item.into());

        write!(
            f,
            [group(&format_args![
                text("["),
                dangling_comments(dangling),
                soft_block_indent(&items),
                text("]")
            ])]
        )
    }

    fn fmt_dangling_comments(&self, _node: &ExprList, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled as part of `fmt_fields`
        Ok(())
    }
}
