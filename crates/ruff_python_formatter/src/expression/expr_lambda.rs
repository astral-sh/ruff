use ruff_formatter::write;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprLambda;
use ruff_text_size::Ranged;

use crate::comments::dangling_comments;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::other::parameters::ParametersParentheses;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprLambda;

impl FormatNodeRule<ExprLambda> for FormatExprLambda {
    fn fmt_fields(&self, item: &ExprLambda, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprLambda {
            range: _,
            node_index: _,
            parameters,
            body,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(f, [token("lambda")])?;

        if let Some(parameters) = parameters {
            // In this context, a dangling comment can either be a comment between the `lambda` the
            // parameters, or a comment between the parameters and the body.
            let (dangling_before_parameters, dangling_after_parameters) = dangling
                .split_at(dangling.partition_point(|comment| comment.end() < parameters.start()));

            if dangling_before_parameters.is_empty() {
                // If the first parameter has a leading comment, insert a hard line break. This
                // comment is associated as a leading comment on the first parameter:
                //
                // ```py
                // (
                //     lambda
                //     * # comment
                //     x:
                //     x
                // )
                // ```
                //
                // so a hard line break is needed to avoid formatting it like:
                //
                // ```py
                // (
                //     lambda # comment
                //     *x: x
                // )
                // ```
                //
                // which is unstable because it's missing the second space before the comment.
                //
                // Inserting the line break causes it to format like:
                //
                // ```py
                // (
                //     lambda
                //     # comment
                //     *x :x
                // )
                // ```
                //
                // which is also consistent with the formatting in the presence of an actual
                // dangling comment on the lambda:
                //
                // ```py
                // (
                //     lambda # comment 1
                //     * # comment 2
                //     x:
                //     x
                // )
                // ```
                //
                // formats to:
                //
                // ```py
                // (
                //     lambda  # comment 1
                //     # comment 2
                //     *x: x
                // )
                // ```
                if comments.has_leading(&**parameters) {
                    hard_line_break().fmt(f)?;
                } else {
                    write!(f, [space()])?;
                }
            } else {
                write!(f, [dangling_comments(dangling_before_parameters)])?;
            }

            write!(
                f,
                [parameters
                    .format()
                    .with_options(ParametersParentheses::Never)]
            )?;

            write!(f, [token(":")])?;

            if dangling_after_parameters.is_empty() {
                write!(f, [space()])?;
            } else {
                write!(f, [dangling_comments(dangling_after_parameters)])?;
            }
        } else {
            write!(f, [token(":")])?;

            // In this context, a dangling comment is a comment between the `lambda` and the body.
            if dangling.is_empty() {
                write!(f, [space()])?;
            } else {
                write!(f, [dangling_comments(dangling)])?;
            }
        }

        write!(f, [body.format()])
    }
}

impl NeedsParentheses for ExprLambda {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() {
            OptionalParentheses::Always
        } else {
            OptionalParentheses::Multiline
        }
    }
}
