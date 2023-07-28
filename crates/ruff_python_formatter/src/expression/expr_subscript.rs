use ruff_formatter::{format_args, write, FormatRuleWithOptions};
use ruff_python_ast::node::{AnyNodeRef, AstNode};
use ruff_python_ast::{Expr, ExprSubscript};

use crate::comments::trailing_comments;
use crate::context::PyFormatContext;
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatExprSubscript {
    fluent_style: bool,
}

impl FormatRuleWithOptions<ExprSubscript, PyFormatContext<'_>> for FormatExprSubscript {
    type Options = bool;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.fluent_style = options;
        self
    }
}

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

        let format_value = format_with(|f| {
            if self.fluent_style {
                match value.as_ref() {
                    Expr::Attribute(expr) => expr.format().with_options(self.fluent_style).fmt(f),
                    Expr::Call(expr) => expr.format().with_options(self.fluent_style).fmt(f),
                    Expr::Subscript(expr) => expr.format().with_options(self.fluent_style).fmt(f),
                    _ => value.format().fmt(f),
                }
            } else {
                value.format().fmt(f)
            }
        });

        if let NodeLevel::Expression(Some(_)) = f.context().node_level() {
            // Enforce the optional parentheses for parenthesized values.
            let mut f = WithNodeLevel::new(NodeLevel::Expression(None), f);
            write!(f, [format_value])?;
        } else {
            format_value.fmt(f)?;
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
