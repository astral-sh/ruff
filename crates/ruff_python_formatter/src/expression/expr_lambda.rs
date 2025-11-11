use ruff_formatter::write;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprLambda;
use ruff_text_size::Ranged;

use crate::comments::dangling_comments;
use crate::comments::leading_comments;
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
                write!(f, [space()])?;
            }

            group(&format_with(|f: &mut PyFormatter| {
                if f.context().node_level().is_parenthesized()
                    && (parameters.len() > 1 || !dangling_before_parameters.is_empty())
                {
                    let end_of_line_start = dangling_before_parameters
                        .partition_point(|comment| comment.line_position().is_end_of_line());
                    let (same_line_comments, own_line_comments) =
                        dangling_before_parameters.split_at(end_of_line_start);

                    dangling_comments(same_line_comments).fmt(f)?;

                    write![
                        f,
                        [
                            soft_line_break(),
                            leading_comments(own_line_comments),
                            parameters
                                .format()
                                .with_options(ParametersParentheses::Never),
                        ]
                    ]
                } else {
                    parameters
                        .format()
                        .with_options(ParametersParentheses::Never)
                        .fmt(f)
                }?;

                write!(f, [token(":")])?;

                if dangling_after_parameters.is_empty() {
                    write!(f, [space()])
                } else {
                    write!(f, [dangling_comments(dangling_after_parameters)])
                }
            }))
            .fmt(f)?;
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
