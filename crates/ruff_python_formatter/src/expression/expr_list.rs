use crate::comments::dangling_comments;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
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
            let mut iter = elts.iter();

            if let Some(first) = iter.next() {
                write!(f, [first.format()])?;
            }

            for item in iter {
                write!(f, [text(","), soft_line_break_or_space(), item.format()])?;
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

impl NeedsParentheses for ExprList {
    fn needs_parentheses(&self, parenthesize: Parenthesize, source: &str) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
