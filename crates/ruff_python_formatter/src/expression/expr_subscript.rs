use ruff_python_ast::{Expr, ExprSubscript};

use ruff_formatter::{format_args, write};
use ruff_python_ast::node::{AnyNodeRef, AstNode};

use crate::comments::trailing_comments;
use crate::context::PyFormatContext;
use crate::context::{NodeLevel, WithNodeLevel};
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
            "A subscript expression can only have a single dangling comment, the one after the bracket"
        );

        if let NodeLevel::Expression(Some(_)) = f.context().node_level() {
            // Enforce the optional parentheses for parenthesized values.
            let mut f = WithNodeLevel::new(NodeLevel::Expression(None), f);
            write!(f, [value.format()])?;
        } else {
            value.format().fmt(f)?;
        }

        let format_slice = format_with(|f: &mut PyFormatter| {
            let mut f = WithNodeLevel::new(NodeLevel::ParenthesizedExpression, f);

            if let Expr::Tuple(tuple) = slice.as_ref() {
                write!(f, [tuple.format().with_options(TupleParentheses::Preserve)])
            } else {
                write!(f, [slice.format()])
            }
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
