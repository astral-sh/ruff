use crate::expression::{format_call_chain_layout, CallChainLayout};
use ruff_formatter::FormatRuleWithOptions;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Expr, ExprCall};

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::FormatNodeRule;

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

        let call_chain_layout = format_call_chain_layout(f, self.call_chain_layout, item);

        let fmt_inner = format_with(|f| {
            match func.as_ref() {
                Expr::Attribute(expr) => expr.format().with_options(call_chain_layout).fmt(f)?,
                Expr::Call(expr) => expr.format().with_options(call_chain_layout).fmt(f)?,
                Expr::Subscript(expr) => expr.format().with_options(call_chain_layout).fmt(f)?,
                _ => func.format().fmt(f)?,
            }

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
            group(&fmt_inner).fmt(f)
        } else {
            fmt_inner.fmt(f)
        }
    }
}

impl NeedsParentheses for ExprCall {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if CallChainLayout::from_expression(self.into(), context.source())
            == CallChainLayout::Fluent
        {
            OptionalParentheses::Multiline
        } else {
            self.func.needs_parentheses(self.into(), context)
        }
    }
}
