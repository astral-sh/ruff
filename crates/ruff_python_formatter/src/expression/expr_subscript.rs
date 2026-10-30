use ruff_formatter::{FormatRuleWithOptions, write};
use ruff_python_ast::ExprSubscript;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;

use crate::expression::CallChainLayout;
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, Parentheses, parenthesized,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprSubscript {
    call_chain_layout: CallChainLayout,
}

impl FormatRuleWithOptions<flat::View<'_, ExprSubscript>, PyFormatContext<'_>>
    for FormatExprSubscript
{
    type Options = CallChainLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.call_chain_layout = options;
        self
    }
}

impl FormatNodeRule<ExprSubscript> for FormatExprSubscript {
    fn fmt_fields(
        &self,
        item: flat::View<'_, ExprSubscript>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let value = item.value();
        let slice = item.slice();

        let call_chain_layout = self.call_chain_layout.apply_in_node(item, f);

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling(item);
        debug_assert!(
            dangling_comments.len() <= 1,
            "A subscript expression can only have a single dangling comment, the one after the bracket"
        );

        let format_inner = format_with(|f: &mut PyFormatter| {
            if f.context().is_expression_parenthesized(value) {
                value.format().with_options(Parentheses::Always).fmt(f)
            } else {
                match value.classify() {
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
                    _ => value.format().with_options(Parentheses::Never).fmt(f),
                }
            }?;

            let format_slice = format_with(|f: &mut PyFormatter| {
                if let flat::ExprKind::Tuple(tuple) = slice.classify() {
                    write!(f, [tuple.format().with_options(TupleParentheses::Preserve)])
                } else {
                    write!(f, [slice.format()])
                }
            });

            parenthesized("[", &format_slice, "]")
                .with_dangling_comments(dangling_comments)
                .fmt(f)
        });

        let is_call_chain_root =
            self.call_chain_layout == CallChainLayout::Default && call_chain_layout.is_fluent();
        if is_call_chain_root {
            write!(f, [group(&format_inner)])
        } else {
            write!(f, [format_inner])
        }
    }
}

impl NeedsParentheses for flat::View<'_, ExprSubscript> {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        {
            if CallChainLayout::from_expression((*self).into(), context).is_fluent() {
                OptionalParentheses::Multiline
            } else if context.is_expression_parenthesized(self.value()) {
                OptionalParentheses::Never
            } else {
                match self.value().needs_parentheses(self.into(), context) {
                    OptionalParentheses::BestFit => {
                        if let Some(function) = parent.as_stmt_function_def() {
                            if function.returns().is_some_and(|returns| {
                                AnyNodeRef::ptr_eq(returns.into(), self.into())
                            }) {
                                if function.parameters().is_empty()
                                    && !context.comments().has(function.parameters())
                                {
                                    // Apply the `optional_parentheses` layout when the subscript
                                    // is in a return type position of a function without parameters.
                                    // This ensures the subscript is parenthesized if it has a very
                                    // long name that goes over the line length limit.
                                    return OptionalParentheses::Multiline;
                                }

                                // Don't use the best fitting layout for return type annotation because it results in the
                                // return type expanding before the parameters.
                                return OptionalParentheses::Never;
                            }
                        }
                        OptionalParentheses::BestFit
                    }
                    parentheses => parentheses,
                }
            }
        }
    }
}
