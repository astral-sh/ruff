use ruff_formatter::{format_args, write, FormatRuleWithOptions};
use ruff_python_ast::node::{AnyNodeRef, AstNode};
use ruff_python_ast::{Expr, ExprSubscript};

use crate::comments::{trailing_comments, SourceComment};

use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::expression::CallChainLayout;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprSubscript {
    call_chain_layout: CallChainLayout,
}

impl FormatRuleWithOptions<ExprSubscript, PyFormatContext<'_>> for FormatExprSubscript {
    type Options = CallChainLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.call_chain_layout = options;
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

        let call_chain_layout = self.call_chain_layout.apply_in_node(item, f);

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling(item.as_any_node_ref());
        debug_assert!(
            dangling_comments.len() <= 1,
            "A subscript expression can only have a single dangling comment, the one after the bracket"
        );

        let format_value = format_with(|f| match value.as_ref() {
            Expr::Attribute(expr) => expr.format().with_options(call_chain_layout).fmt(f),
            Expr::Call(expr) => expr.format().with_options(call_chain_layout).fmt(f),
            Expr::Subscript(expr) => expr.format().with_options(call_chain_layout).fmt(f),
            _ => value.format().fmt(f),
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
                token("["),
                trailing_comments(dangling_comments),
                soft_block_indent(&format_slice),
                token("]")
            ])]
        )
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
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
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        {
            if CallChainLayout::from_expression(self.into(), context.source())
                == CallChainLayout::Fluent
            {
                OptionalParentheses::Multiline
            } else {
                match self.value.needs_parentheses(self.into(), context) {
                    OptionalParentheses::BestFit => OptionalParentheses::Never,
                    parentheses => parentheses,
                }
            }
        }
    }
}
