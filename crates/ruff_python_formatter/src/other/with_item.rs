use rustpython_parser::ast::WithItem;

use ruff_formatter::{write, Buffer, FormatResult};

use crate::comments::trailing_comments;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatWithItem;

impl FormatNodeRule<WithItem> for FormatWithItem {
    fn fmt_fields(&self, item: &WithItem, f: &mut PyFormatter) -> FormatResult<()> {
        let WithItem {
            range: _,
            context_expr,
            optional_vars,
        } = item;

        let comments = f.context().comments().clone();
        let trailing_as_comments = comments.dangling_comments(item);

        maybe_parenthesize_expression(context_expr, item, Parenthesize::IfRequired).fmt(f)?;

        if let Some(optional_vars) = optional_vars {
            write!(
                f,
                [
                    space(),
                    text("as"),
                    trailing_comments(trailing_as_comments),
                    space(),
                    optional_vars.format(),
                ]
            )?;
        }
        Ok(())
    }

    fn fmt_dangling_comments(&self, _node: &WithItem, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
