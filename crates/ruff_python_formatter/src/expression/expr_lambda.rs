use ruff_formatter::{format_args, write};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprLambda;
use ruff_text_size::Ranged;

use crate::comments::{dangling_comments, leading_comments};
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parenthesize};
use crate::expression::{has_own_parentheses, maybe_parenthesize_expression};
use crate::other::parameters::ParametersParentheses;
use crate::prelude::*;
use crate::preview::is_indent_lambda_parameters_enabled;
use crate::preview::is_parenthesize_lambda_bodies_enabled;

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
            // In this context, a dangling comment can either be a comment between the `lambda` and the
            // parameters, or a comment between the parameters and the body.
            let (dangling_before_parameters, dangling_after_parameters) = dangling
                .split_at(dangling.partition_point(|comment| comment.end() < parameters.start()));

            if is_indent_lambda_parameters_enabled(f.context()) {
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

                        soft_block_indent(&format_args![
                            leading_comments(own_line_comments),
                            parameters
                                .format()
                                .with_options(ParametersParentheses::Never),
                        ])
                        .fmt(f)
                    } else {
                        parameters
                            .format()
                            .with_options(ParametersParentheses::Never)
                            .fmt(f)
                    }?;

                    token(":").fmt(f)?;

                    if dangling_after_parameters.is_empty() {
                        space().fmt(f)
                    } else {
                        dangling_comments(dangling_after_parameters).fmt(f)
                    }
                }))
                .fmt(f)?;
            } else {
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

        // Avoid parenthesizing lists, dictionaries, etc.
        if is_parenthesize_lambda_bodies_enabled(f.context())
            && has_own_parentheses(body, f.context()).is_none()
        {
            maybe_parenthesize_expression(body, item, Parenthesize::IfBreaksParenthesizedNested)
                .fmt(f)
        } else {
            body.format().fmt(f)
        }
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
