use ruff_formatter::FormatRuleWithOptions;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{Expr, ExprCall};

use crate::comments::dangling_comments;
use crate::expression::parentheses::{
    is_expression_parenthesized, NeedsParentheses, OptionalParentheses, Parentheses,
};
use crate::expression::CallChainLayout;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprCall {
    call_chain_layout: CallChainLayout,
}

impl FormatRuleWithOptions<ExprCall, PyFormatContext<'_>> for FormatExprCall {
    type Options = CallChainLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.call_chain_layout = options;
        self
    }
}

impl FormatNodeRule<ExprCall> for FormatExprCall {
    fn fmt_fields(&self, item: &ExprCall, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprCall {
            range: _,
            func,
            arguments,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        let call_chain_layout = self.call_chain_layout.apply_in_node(item, f);

        let fmt_func = format_with(|f: &mut PyFormatter| {
            // Format the function expression.
            if is_expression_parenthesized(
                func.into(),
                f.context().comments().ranges(),
                f.context().source(),
            ) {
                func.format().with_options(Parentheses::Always).fmt(f)
            } else {
                match func.as_ref() {
                    Expr::Attribute(expr) => expr.format().with_options(call_chain_layout).fmt(f),
                    Expr::Call(expr) => expr.format().with_options(call_chain_layout).fmt(f),
                    Expr::Subscript(expr) => expr.format().with_options(call_chain_layout).fmt(f),
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
        if call_chain_layout == CallChainLayout::Fluent
            && self.call_chain_layout == CallChainLayout::Default
        {
            group(&fmt_func).fmt(f)
        } else {
            fmt_func.fmt(f)
        }
    }
}

impl NeedsParentheses for ExprCall {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if CallChainLayout::from_expression(
            self.into(),
            context.comments().ranges(),
            context.source(),
        ) == CallChainLayout::Fluent
        {
            OptionalParentheses::Multiline
        } else if context.comments().has_dangling(self) {
            OptionalParentheses::Always
        } else if is_expression_parenthesized(
            self.func.as_ref().into(),
            context.comments().ranges(),
            context.source(),
        ) {
            OptionalParentheses::Never
        } else {
            self.func.needs_parentheses(self.into(), context)
        }
    }
}
