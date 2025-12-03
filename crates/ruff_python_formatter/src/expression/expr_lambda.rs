use ruff_formatter::{FormatRuleWithOptions, RemoveSoftLinesBuffer, format_args, write};
use ruff_python_ast::{AnyNodeRef, Expr, ExprLambda};
use ruff_text_size::Ranged;

use crate::builders::parenthesize_if_expands;
use crate::comments::dangling_comments;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parentheses};
use crate::expression::{CallChainLayout, has_own_parentheses};
use crate::other::parameters::ParametersParentheses;
use crate::prelude::*;
use crate::preview::is_force_single_line_lambda_parameters_enabled;
use crate::preview::is_parenthesize_lambda_bodies_enabled;

#[derive(Default)]
pub struct FormatExprLambda {
    layout: ExprLambdaLayout,
}

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

            if dangling_before_parameters.is_empty() {
                write!(f, [space()])?;
            } else {
                write!(f, [dangling_comments(dangling_before_parameters)])?;
            }

            // Try to keep the parameters on a single line, unless there are intervening comments.
            if is_force_single_line_lambda_parameters_enabled(f.context())
                && !comments.contains_comments(parameters.as_ref().into())
            {
                let mut buffer = RemoveSoftLinesBuffer::new(f);
                write!(
                    buffer,
                    [parameters
                        .format()
                        .with_options(ParametersParentheses::Never)]
                )?;
            } else {
                write!(
                    f,
                    [parameters
                        .format()
                        .with_options(ParametersParentheses::Never)]
                )?;
            }

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

        if is_parenthesize_lambda_bodies_enabled(f.context()) {
            let fmt_body = format_with(|f: &mut PyFormatter| {
                if matches!(&**body, Expr::Call(_) | Expr::Subscript(_)) {
                    let unparenthesized = body.format().with_options(Parentheses::Never).memoized();
                    if CallChainLayout::from_expression(
                        body.into(),
                        comments.ranges(),
                        f.context().source(),
                    ) == CallChainLayout::Fluent
                    {
                        parenthesize_if_expands(&unparenthesized).fmt(f)
                    } else {
                        best_fitting![
                            // body all flat
                            unparenthesized,
                            // body expanded
                            group(&unparenthesized).should_expand(true),
                            // parenthesized
                            format_args![token("("), block_indent(&unparenthesized), token(")")]
                        ]
                        .fmt(f)
                    }
                } else if has_own_parentheses(body, f.context()).is_some()
                    || comments.contains_comments(body.as_ref().into())
                {
                    body.format().fmt(f)
                } else {
                    parenthesize_if_expands(&body.format().with_options(Parentheses::Never)).fmt(f)
                }
            });

            match self.layout {
                // Can we move the `fits_expanded` into the assignment formatting?
                ExprLambdaLayout::Assignment => fits_expanded(&fmt_body).fmt(f),
                ExprLambdaLayout::Default => fmt_body.fmt(f),
            }
        } else {
            body.format().fmt(f)
        }
    }
}

#[derive(Debug, Default, Copy, Clone)]
pub enum ExprLambdaLayout {
    #[default]
    Default,

    /// The [`ExprLambda`] is the direct child of an assignment expression, so it needs to use
    /// `fits_expanded` to prefer parenthesizing its own body before the assignment tries to
    /// parenthesize the whole lambda. For example, we want this formatting:
    ///
    /// ```py
    /// long_assignment_target = lambda x, y, z: (
    ///     x + y + z
    /// )
    /// ```
    ///
    /// instead of either of these:
    ///
    /// ```py
    /// long_assignment_target = (
    ///     lambda x, y, z: (
    ///         x + y + z
    ///     )
    /// )
    ///
    /// long_assignment_target = (
    ///     lambda x, y, z: x + y + z
    /// )
    /// ```
    Assignment,
}

impl FormatRuleWithOptions<ExprLambda, PyFormatContext<'_>> for FormatExprLambda {
    type Options = ExprLambdaLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.layout = options;
        self
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
