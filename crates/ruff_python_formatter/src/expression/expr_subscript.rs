use rustpython_parser::ast::{Expr, ExprSubscript};

use ruff_formatter::{format_args, write};
use ruff_python_ast::node::{AnyNodeRef, AstNode};

use crate::comments::trailing_comments;
use crate::context::NodeLevel;
use crate::context::PyFormatContext;
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
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

        let format_slice = format_with(|f: &mut PyFormatter| {
            let saved_level = f.context().node_level();
            f.context_mut()
                .set_node_level(NodeLevel::ParenthesizedExpression);

            let result = if let Expr::Tuple(tuple) = slice.as_ref() {
                tuple
                    .format()
                    .with_options(TupleParentheses::Subscript)
                    .fmt(f)
            } else {
                slice.format().fmt(f)
            };

            f.context_mut().set_node_level(saved_level);

            result
        });

        write!(
            f,
            [group(&format_args![
                text("["),
                trailing_comments(dangling_comments),
                soft_block_indent(&format_slice),
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
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        {
            OptionalParentheses::Never
        }
    }
}
