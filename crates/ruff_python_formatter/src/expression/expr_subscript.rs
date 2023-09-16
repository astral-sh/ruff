use ruff_formatter::{write, FormatRuleWithOptions};
use ruff_python_ast::node::{AnyNodeRef, AstNode};
use ruff_python_ast::{Expr, ExprSubscript};

use crate::comments::SourceComment;
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::parentheses::{
    is_expression_parenthesized, parenthesized, NeedsParentheses, OptionalParentheses,
};
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

        let format_inner = format_with(|f| {
            match value.as_ref() {
                Expr::Attribute(expr) => expr.format().with_options(call_chain_layout).fmt(f)?,
                Expr::Call(expr) => expr.format().with_options(call_chain_layout).fmt(f)?,
                Expr::Subscript(expr) => expr.format().with_options(call_chain_layout).fmt(f)?,
                _ => value.format().fmt(f)?,
            }

            let format_slice = format_with(|f: &mut PyFormatter| {
                if let Expr::Tuple(tuple) = slice.as_ref() {
                    write!(f, [tuple.format().with_options(TupleParentheses::Preserve)])
                } else {
                    write!(f, [slice.format()])
                }
            });

            parenthesized("[", &format_slice, "]")
                .with_dangling_comments(dangling_comments)
                .fmt(f)
        });

        let is_call_chain_root = self.call_chain_layout == CallChainLayout::Default
            && call_chain_layout == CallChainLayout::Fluent;
        if is_call_chain_root {
            write!(f, [group(&format_inner)])
        } else {
            write!(f, [format_inner])
        }
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
            } else if is_expression_parenthesized(self.value.as_ref().into(), context.source()) {
                OptionalParentheses::Never
            } else {
                match self.value.needs_parentheses(self.into(), context) {
                    OptionalParentheses::BestFit => OptionalParentheses::Never,
                    parentheses => parentheses,
                }
            }
        }
    }
}
