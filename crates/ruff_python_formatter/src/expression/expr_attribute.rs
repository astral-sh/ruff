use ruff_formatter::{write, FormatRuleWithOptions};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{Expr, ExprAttribute, ExprNumberLiteral, Number};
use ruff_python_trivia::{find_only_token_in_range, SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::comments::dangling_comments;
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
                is_base_ten_number_literal(value.as_ref(), f.context().source()) || {
                    is_expression_parenthesized(
                        value.into(),
                        f.context().comments().ranges(),
                        f.context().source(),
                    )
                };

            if call_chain_layout == CallChainLayout::Fluent {
                if parenthesize_value {
                    // Don't propagate the call chain layout.
                    value.format().with_options(Parentheses::Always).fmt(f)?;
                } else {
                    match value.as_ref() {
                        Expr::Attribute(expr) => {
                            expr.format().with_options(call_chain_layout).fmt(f)?;
                        }
                        Expr::Call(expr) => {
                            expr.format().with_options(call_chain_layout).fmt(f)?;
                        }
                        Expr::Subscript(expr) => {
                            expr.format().with_options(call_chain_layout).fmt(f)?;
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

            let comments = f.context().comments().clone();

            // Always add a line break if the value is parenthesized and there's an
            // end of line comment on the same line as the closing parenthesis.
            // ```python
            // (
            //      (
            //          a
            //      )  # `end_of_line_comment`
            //      .
            //      b
            // )
            // ```
            let has_trailing_end_of_line_comment =
                SimpleTokenizer::starts_at(value.end(), f.context().source())
                    .skip_trivia()
                    .take_while(|token| token.kind == SimpleTokenKind::RParen)
                    .last()
                    .is_some_and(|right_paren| {
                        let trailing_value_comments = comments.trailing(&**value);
                        trailing_value_comments.iter().any(|comment| {
                            comment.line_position().is_end_of_line()
                                && comment.start() > right_paren.end()
                        })
                    });

            if has_trailing_end_of_line_comment {
                hard_line_break().fmt(f)?;
            }
            // Allow the `.` on its own line if this is a fluent call chain
            // and the value either requires parenthesizing or is a call or subscript expression
            // (it's a fluent chain but not the first element).
            else if call_chain_layout == CallChainLayout::Fluent {
                if parenthesize_value || value.is_call_expr() || value.is_subscript_expr() {
                    soft_line_break().fmt(f)?;
                }
            }

            // Identify dangling comments before and after the dot:
            // ```python
            // (
            //      (
            //          a
            //      )
            //      # `before_dot`
            //      .  # `after_dot`
            //      # `after_dot`
            //      b
            // )
            // ```
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
}

impl NeedsParentheses for ExprAttribute {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        // Checks if there are any own line comments in an attribute chain (a.b.c).
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
            self.value.as_ref().into(),
            context.comments().ranges(),
            context.source(),
        ) {
            OptionalParentheses::Never
        } else {
            self.value.needs_parentheses(self.into(), context)
        }
    }
}

// Non Hex, octal or binary number literals need parentheses to disambiguate the attribute `.` from
// a decimal point. Floating point numbers don't strictly need parentheses but it reads better (rather than 0.0.test()).
fn is_base_ten_number_literal(expr: &Expr, source: &str) -> bool {
    if let Some(ExprNumberLiteral { value, range }) = expr.as_number_literal_expr() {
        match value {
            Number::Float(_) => true,
            Number::Int(_) => {
                let text = &source[*range];
                !matches!(
                    text.as_bytes().get(0..2),
                    Some([b'0', b'x' | b'X' | b'o' | b'O' | b'b' | b'B'])
                )
            }
            Number::Complex { .. } => false,
        }
    } else {
        false
    }
}
