use ruff_formatter::{write, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Constant, Expr, ExprAttribute, ExprConstant};
use ruff_python_trivia::{find_only_token_in_range, SimpleTokenKind};
use ruff_text_size::{Ranged, TextRange};

use crate::comments::{dangling_comments, SourceComment};
use crate::expression::parentheses::{
    is_expression_parenthesized, NeedsParentheses, OptionalParentheses, Parentheses,
};
use crate::expression::CallChainLayout;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprAttribute {
    call_chain_layout: CallChainLayout,
}

impl FormatRuleWithOptions<ExprAttribute, PyFormatContext<'_>> for FormatExprAttribute {
    type Options = CallChainLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.call_chain_layout = options;
        self
    }
}

impl FormatNodeRule<ExprAttribute> for FormatExprAttribute {
    fn fmt_fields(&self, item: &ExprAttribute, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprAttribute {
            value,
            range: _,
            attr,
            ctx: _,
        } = item;

        let call_chain_layout = self.call_chain_layout.apply_in_node(item, f);

        let format_inner = format_with(|f: &mut PyFormatter| {
            let parenthesize_value =
                // If the value is an integer, we need to parenthesize it to avoid a syntax error.
                matches!(
                    value.as_ref(),
                    Expr::Constant(ExprConstant {
                        value: Constant::Int(_) | Constant::Float(_),
                        ..
                    })
                ) || is_expression_parenthesized(value.into(), f.context().source());

            if call_chain_layout == CallChainLayout::Fluent {
                if parenthesize_value {
                    // Don't propagate the call chain layout.
                    value.format().with_options(Parentheses::Always).fmt(f)?;

                    // Format the dot on its own line.
                    soft_line_break().fmt(f)?;
                } else {
                    match value.as_ref() {
                        Expr::Attribute(expr) => {
                            expr.format().with_options(call_chain_layout).fmt(f)?;
                        }
                        Expr::Call(expr) => {
                            expr.format().with_options(call_chain_layout).fmt(f)?;
                            soft_line_break().fmt(f)?;
                        }
                        Expr::Subscript(expr) => {
                            expr.format().with_options(call_chain_layout).fmt(f)?;
                            soft_line_break().fmt(f)?;
                        }
                        _ => {
                            value.format().with_options(Parentheses::Never).fmt(f)?;
                        }
                    }
                }
            } else if parenthesize_value {
                value.format().with_options(Parentheses::Always).fmt(f)?;
            } else {
                value.format().with_options(Parentheses::Never).fmt(f)?;
            }

            // Identify dangling comments before and after the dot:
            // ```python
            // (
            //      (
            //          a
            //      )  # `before_dot`
            //      # `before_dot`
            //      .  # `after_dot`
            //      # `after_dot`
            //      b
            // )
            // ```
            let comments = f.context().comments().clone();
            let dangling = comments.dangling(item);
            let (before_dot, after_dot) = if dangling.is_empty() {
                (dangling, dangling)
            } else {
                let dot_token = find_only_token_in_range(
                    TextRange::new(item.value.end(), item.attr.start()),
                    SimpleTokenKind::Dot,
                    f.context().source(),
                );
                dangling.split_at(
                    dangling.partition_point(|comment| comment.start() < dot_token.start()),
                )
            };

            write!(
                f,
                [
                    dangling_comments(before_dot),
                    token("."),
                    dangling_comments(after_dot),
                    attr.format()
                ]
            )
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
        // handle in `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprAttribute {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        // Checks if there are any own line comments in an attribute chain (a.b.c).
        if CallChainLayout::from_expression(self.into(), context.source())
            == CallChainLayout::Fluent
        {
            OptionalParentheses::Multiline
        } else if context.comments().has_dangling(self) {
            OptionalParentheses::Always
        } else if self.value.is_name_expr() {
            OptionalParentheses::BestFit
        } else if is_expression_parenthesized(self.value.as_ref().into(), context.source()) {
            OptionalParentheses::Never
        } else {
            self.value.needs_parentheses(self.into(), context)
        }
    }
}
