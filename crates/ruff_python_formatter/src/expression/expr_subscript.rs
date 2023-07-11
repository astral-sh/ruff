use rustpython_parser::ast::ExprSubscript;

use ruff_formatter::{format_args, write};
use ruff_python_ast::node::AstNode;

use crate::comments::trailing_comments;
use crate::context::NodeLevel;
use crate::context::PyFormatContext;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::prelude::*;
use crate::FormatNodeRule;

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

        if let NodeLevel::Expression(Some(group_id)) = f.context().node_level() {
            // Enforce the optional parentheses for parenthesized values.
            f.context_mut().set_node_level(NodeLevel::Expression(None));
            let result = value.format().fmt(f);
            f.context_mut()
                .set_node_level(NodeLevel::Expression(Some(group_id)));
            result?;
        } else {
            value.format().fmt(f)?;
        }

        write!(
            f,
            [group(&format_args![
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
        context: &PyFormatContext,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, context) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
