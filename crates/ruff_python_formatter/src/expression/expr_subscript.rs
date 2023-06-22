use crate::comments::{trailing_comments, Comments};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{group, soft_block_indent, text};
use ruff_formatter::{format_args, write, Buffer, FormatResult};
use ruff_python_ast::node::AstNode;
use rustpython_parser::ast::ExprSubscript;

#[derive(Default)]
pub struct FormatExprSubscript;

impl FormatNodeRule<ExprSubscript> for FormatExprSubscript {
    fn fmt_fields(&self, item: &ExprSubscript, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprSubscript {
            range: _,
            value,
            slice,
            ctx: _,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling_comments(item.as_any_node_ref());
        debug_assert!(
            dangling_comments.len() <= 1,
            "The subscript expression must have at most a single comment, the one after the bracket"
        );

        write!(
            f,
            [group(&format_args![
                value.format(),
                text("["),
                trailing_comments(dangling_comments),
                soft_block_indent(&slice.format()),
                text("]")
            ])]
        )
    }

    fn fmt_dangling_comments(
        &self,
        _node: &ExprSubscript,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled inside of `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprSubscript {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
