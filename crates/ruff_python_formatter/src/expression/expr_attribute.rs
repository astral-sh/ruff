use ruff_formatter::{write, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Constant, Expr, ExprAttribute, ExprConstant};

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parentheses};
use crate::expression::CallChainLayout;
use crate::prelude::*;
use crate::FormatNodeRule;

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

        let call_chain_layout = match self.call_chain_layout {
            CallChainLayout::Default => {
                if f.context().node_level().is_parenthesized() {
                    CallChainLayout::from_expression(AnyNodeRef::from(item), f.context().source())
                } else {
                    CallChainLayout::NonFluent
                }
            }
            layout @ (CallChainLayout::Fluent | CallChainLayout::NonFluent) => layout,
        };

        let needs_parentheses = matches!(
            value.as_ref(),
            Expr::Constant(ExprConstant {
                value: Constant::Int(_) | Constant::Float(_),
                ..
            })
        );

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling_comments(item);
        let leading_attribute_comments_start =
            dangling_comments.partition_point(|comment| comment.line_position().is_end_of_line());
        let (trailing_dot_comments, leading_attribute_comments) =
            dangling_comments.split_at(leading_attribute_comments_start);

        if needs_parentheses {
            value.format().with_options(Parentheses::Always).fmt(f)?;
        } else {
            match value.as_ref() {
                Expr::Attribute(expr) => {
                    expr.format().with_options(call_chain_layout).fmt(f)?;
                }
                Expr::Call(expr) => {
                    expr.format().with_options(call_chain_layout).fmt(f)?;
                    if call_chain_layout == CallChainLayout::Fluent {
                        // Format the dot on its own line
                        soft_line_break().fmt(f)?;
                    }
                }
                Expr::Subscript(expr) => {
                    expr.format().with_options(call_chain_layout).fmt(f)?;
                    if call_chain_layout == CallChainLayout::Fluent {
                        // Format the dot on its own line
                        soft_line_break().fmt(f)?;
                    }
                }
                _ => value.format().fmt(f)?,
            }
        }

        if comments.has_trailing_own_line_comments(value.as_ref()) {
            hard_line_break().fmt(f)?;
        }

        if call_chain_layout == CallChainLayout::Fluent {
            // Fluent style has line breaks before the dot
            // ```python
            // blogs3 = (
            //     Blog.objects.filter(
            //         entry__headline__contains="Lennon",
            //     )
            //     .filter(
            //         entry__pub_date__year=2008,
            //     )
            //     .filter(
            //         entry__pub_date__year=2008,
            //     )
            // )
            // ```
            write!(
                f,
                [
                    (!leading_attribute_comments.is_empty()).then_some(hard_line_break()),
                    leading_comments(leading_attribute_comments),
                    text("."),
                    trailing_comments(trailing_dot_comments),
                    attr.format()
                ]
            )
        } else {
            // Regular style
            // ```python
            // blogs2 = Blog.objects.filter(
            //     entry__headline__contains="Lennon",
            // ).filter(
            //     entry__pub_date__year=2008,
            // )
            // ```
            write!(
                f,
                [
                    text("."),
                    trailing_comments(trailing_dot_comments),
                    (!leading_attribute_comments.is_empty()).then_some(hard_line_break()),
                    leading_comments(leading_attribute_comments),
                    attr.format()
                ]
            )
        }
    }

    fn fmt_dangling_comments(
        &self,
        _node: &ExprAttribute,
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
        } else if context
            .comments()
            .dangling_comments(self)
            .iter()
            .any(|comment| comment.line_position().is_own_line())
            || context.comments().has_trailing_own_line_comments(self)
        {
            OptionalParentheses::Always
        } else {
            self.value.needs_parentheses(self.into(), context)
        }
    }
}
