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
                write!(f, [space()])?;
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
