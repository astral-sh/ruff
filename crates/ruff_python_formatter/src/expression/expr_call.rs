use ruff_formatter::FormatRuleWithOptions;
use ruff_python_ast::ExprCall;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;

use crate::comments::dangling_comments;
use crate::expression::CallChainLayout;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprCall {
    call_chain_layout: CallChainLayout,
}

impl FormatRuleWithOptions<flat::View<'_, ExprCall>, PyFormatContext<'_>> for FormatExprCall {
    type Options = CallChainLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.call_chain_layout = options;
        self
    }
}

impl FormatNodeRule<ExprCall> for FormatExprCall {
    fn fmt_fields(&self, item: flat::View<'_, ExprCall>, f: &mut PyFormatter) -> FormatResult<()> {
        let func = item.func();
        let arguments = item.arguments();

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        let call_chain_layout = self.call_chain_layout.apply_in_node(item, f);

        let fmt_func = format_with(|f: &mut PyFormatter| {
            // Format the function expression.
            if f.context().is_expression_parenthesized(func) {
                func.format().with_options(Parentheses::Always).fmt(f)
            } else {
                match func.classify() {
                    flat::ExprKind::Attribute(expr) => expr
                        .format()
                        .with_options(call_chain_layout.decrement_call_like_count())
                        .fmt(f),
                    flat::ExprKind::Call(expr) => {
                        expr.format().with_options(call_chain_layout).fmt(f)
                    }
                    flat::ExprKind::Subscript(expr) => {
                        expr.format().with_options(call_chain_layout).fmt(f)
                    }
                    _ => func.format().with_options(Parentheses::Never).fmt(f),
                }
            }?;

            // Format comments between the function and its arguments.
            dangling_comments(dangling).fmt(f)?;

            // Format the arguments.
            arguments.format().fmt(f)
        });

        // Allow to indent the parentheses while
        // ```python
        // g1 = (
        //     queryset.distinct().order_by(field.name).values_list(field_name_flat_long_long=True)
        // )
        // ```
        if call_chain_layout.is_fluent() && self.call_chain_layout == CallChainLayout::Default {
            group(&fmt_func).fmt(f)
        } else {
            fmt_func.fmt(f)
        }
    }
}

impl NeedsParentheses for flat::View<'_, ExprCall> {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if CallChainLayout::from_expression((*self).into(), context).is_fluent() {
            OptionalParentheses::Multiline
        } else if context.comments().has_dangling(*self) {
            OptionalParentheses::Always
        } else if context.is_expression_parenthesized(self.func()) {
            OptionalParentheses::Never
        } else {
            self.func().needs_parentheses(self.into(), context)
        }
    }
}
