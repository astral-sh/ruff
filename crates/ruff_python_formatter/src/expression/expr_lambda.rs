use ruff_formatter::{FormatRuleWithOptions, RemoveSoftLinesBuffer, format_args, write};
use ruff_python_ast::{AnyNodeRef, Expr, ExprLambda};
use ruff_text_size::Ranged;

use crate::builders::parenthesize_if_expands;
use crate::comments::{SourceComment, dangling_comments, leading_comments, trailing_comments};
use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, Parentheses, is_expression_parenthesized,
};
use crate::expression::{CallChainLayout, has_own_parentheses};
use crate::other::parameters::ParametersParentheses;
use crate::prelude::*;
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

        let body = &**body;
        let parameters = parameters.as_deref();

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);
        let preview = is_parenthesize_lambda_bodies_enabled(f.context());

        write!(f, [token("lambda")])?;

        if let Some(parameters) = parameters {
            let parameters_have_comments = comments.contains_comments(parameters.into());

            // In this context, a dangling comment can either be a comment between the `lambda` and the
            // parameters, or a comment between the parameters and the body.
            let (dangling_before_parameters, dangling_after_parameters) = dangling
                .split_at(dangling.partition_point(|comment| comment.end() < parameters.start()));

            if dangling_before_parameters.is_empty() {
                // If the parameters have a leading comment, insert a hard line break. This
                // comment is associated as a leading comment on the parameters:
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
                if comments.has_leading(parameters) {
                    hard_line_break().fmt(f)?;
                } else {
                    write!(f, [space()])?;
                }
            } else {
                write!(f, [dangling_comments(dangling_before_parameters)])?;
            }

            // Try to keep the parameters on a single line, unless there are intervening comments.
            if preview && !parameters_have_comments {
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
            }
            // In preview, always parenthesize the body if there are dangling comments.
            else if preview {
                return format_body(body, dangling_after_parameters, self.layout).fmt(f);
            } else {
                write!(f, [dangling_comments(dangling_after_parameters)])?;
            }
        } else {
            write!(f, [token(":")])?;

            // In this context, a dangling comment is a comment between the `lambda` and the body.
            if dangling.is_empty() {
                write!(f, [space()])?;
            }
            // In preview, always parenthesize the body if there are dangling comments.
            else if preview {
                return format_body(body, dangling, self.layout).fmt(f);
            } else {
                write!(f, [dangling_comments(dangling)])?;
            }
        }

        if preview {
            let body_comments = comments.leading_dangling_trailing(body);
            let fmt_body = format_with(|f: &mut PyFormatter| {
                // If the body has comments, we always want to preserve the parentheses. This also
                // ensures that we correctly handle parenthesized comments, and don't need to worry
                // about them in the implementation below.
                if body_comments.has_leading() || body_comments.has_trailing_own_line() {
                    body.format().with_options(Parentheses::Always).fmt(f)
                }
                // Calls and subscripts require special formatting because they have their own
                // parentheses, but they can also have an arbitrary amount of text before the
                // opening parenthesis. We want to avoid cases where we keep a long callable on the
                // same line as the lambda parameters. For example, `db_evmtx...` in:
                //
                // ```py
                // transaction_count = self._query_txs_for_range(
                //     get_count_fn=lambda from_ts, to_ts, _chain_id=chain_id: db_evmtx.count_transactions_in_range(
                //         chain_id=_chain_id,
                //         from_ts=from_ts,
                //         to_ts=to_ts,
                //     ),
                // )
                // ```
                //
                // should cause the whole lambda body to be parenthesized instead:
                //
                // ```py
                // transaction_count = self._query_txs_for_range(
                //     get_count_fn=lambda from_ts, to_ts, _chain_id=chain_id: (
                //         db_evmtx.count_transactions_in_range(
                //             chain_id=_chain_id,
                //             from_ts=from_ts,
                //             to_ts=to_ts,
                //         )
                //     ),
                // )
                // ```
                else if matches!(body, Expr::Call(_) | Expr::Subscript(_)) {
                    let unparenthesized = body.format().with_options(Parentheses::Never);
                    if CallChainLayout::from_expression(
                        body.into(),
                        comments.ranges(),
                        f.context().source(),
                    ) == CallChainLayout::Fluent
                    {
                        parenthesize_if_expands(&unparenthesized).fmt(f)
                    } else {
                        let unparenthesized = unparenthesized.memoized();
                        if unparenthesized.inspect(f)?.will_break() {
                            expand_parent().fmt(f)?;
                        }

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
                }
                // For other cases with their own parentheses, such as lists, sets, dicts, tuples,
                // etc., we can just format the body directly. Their own formatting results in the
                // lambda being formatted well too. For example:
                //
                // ```py
                // lambda xxxxxxxxxxxxxxxxxxxx, yyyyyyyyyyyyyyyyyyyy, zzzzzzzzzzzzzzzzzzzz: [xxxxxxxxxxxxxxxxxxxx, yyyyyyyyyyyyyyyyyyyy, zzzzzzzzzzzzzzzzzzzz]
                // ```
                //
                // gets formatted as:
                //
                // ```py
                // lambda xxxxxxxxxxxxxxxxxxxx, yyyyyyyyyyyyyyyyyyyy, zzzzzzzzzzzzzzzzzzzz: [
                //     xxxxxxxxxxxxxxxxxxxx,
                //     yyyyyyyyyyyyyyyyyyyy,
                //     zzzzzzzzzzzzzzzzzzzz
                // ]
                // ```
                else if has_own_parentheses(body, f.context()).is_some() {
                    body.format().fmt(f)
                }
                // Finally, for expressions without their own parentheses, use
                // `parenthesize_if_expands` to add parentheses around the body, only if it expands
                // across multiple lines. The `Parentheses::Never` here also removes unnecessary
                // parentheses around lambda bodies that fit on one line. For example:
                //
                // ```py
                // lambda xxxxxxxxxxxxxxxxxxxx, yyyyyyyyyyyyyyyyyyyy, zzzzzzzzzzzzzzzzzzzz: xxxxxxxxxxxxxxxxxxxx + yyyyyyyyyyyyyyyyyyyy + zzzzzzzzzzzzzzzzzzzz
                // ```
                //
                // is formatted as:
                //
                // ```py
                // lambda xxxxxxxxxxxxxxxxxxxx, yyyyyyyyyyyyyyyyyyyy, zzzzzzzzzzzzzzzzzzzz: (
                //     xxxxxxxxxxxxxxxxxxxx + yyyyyyyyyyyyyyyyyyyy + zzzzzzzzzzzzzzzzzzzz
                // )
                // ```
                //
                // while
                //
                // ```py
                // lambda xxxxxxxxxxxxxxxxxxxx: (xxxxxxxxxxxxxxxxxxxx + 1)
                // ```
                //
                // is formatted as:
                //
                // ```py
                // lambda xxxxxxxxxxxxxxxxxxxx: xxxxxxxxxxxxxxxxxxxx + 1
                // ```
                else {
                    parenthesize_if_expands(&body.format().with_options(Parentheses::Never)).fmt(f)
                }
            });

            match self.layout {
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

fn format_body<'a>(
    body: &'a Expr,
    dangling: &'a [SourceComment],
    layout: ExprLambdaLayout,
) -> FormatBody<'a> {
    FormatBody {
        body,
        dangling,
        layout,
    }
}

struct FormatBody<'a> {
    body: &'a Expr,
    dangling: &'a [SourceComment],
    layout: ExprLambdaLayout,
}

impl Format<PyFormatContext<'_>> for FormatBody<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let FormatBody {
            dangling,
            body,
            layout,
        } = self;

        // Can't use partition_point because there can be additional end of line comments after the
        // initial set. All of these comments are dangling, for example:
        //
        // ```python
        // (
        //     lambda  # 1
        //     # 2
        //     :  # 3
        //     # 4
        //     y
        // )
        // ```
        //
        // and alternate between own line and end of line.
        let (after_parameters_end_of_line, leading_body_comments) = dangling.split_at(
            dangling
                .iter()
                .position(|comment| comment.line_position().is_own_line())
                .unwrap_or(dangling.len()),
        );

        let fmt_body = format_with(|f: &mut PyFormatter| {
            // If the body is parenthesized and has its own leading comments, preserve the
            // separation between the dangling lambda comments and the body comments. For example,
            // preserve this comment positioning:
            //
            // ```python
            // (
            //      lambda:  # 1
            //      # 2
            //      (  # 3
            //          x
            //      )
            // )
            // ```
            //
            // 1 and 2 are dangling on the lambda and emitted first, followed by a hard line break
            // and the parenthesized body with its leading comments.
            //
            // However, when removing 2, 1 and 3 can instead be formatted on the same line:
            //
            // ```python
            // (
            //      lambda: (  # 1  # 3
            //          x
            //      )
            // )
            // ```
            let comments = f.context().comments();
            if is_expression_parenthesized((*body).into(), comments.ranges(), f.context().source())
                && comments.has_leading(*body)
            {
                if leading_body_comments.is_empty() {
                    write!(
                        f,
                        [
                            space(),
                            trailing_comments(dangling),
                            body.format().with_options(Parentheses::Always),
                        ]
                    )
                } else {
                    write!(
                        f,
                        [
                            trailing_comments(dangling),
                            hard_line_break(),
                            body.format().with_options(Parentheses::Always)
                        ]
                    )
                }
            } else {
                write!(
                    f,
                    [
                        space(),
                        token("("),
                        trailing_comments(after_parameters_end_of_line),
                        block_indent(&format_args!(
                            leading_comments(leading_body_comments),
                            body.format().with_options(Parentheses::Never)
                        )),
                        token(")")
                    ]
                )
            }
        });

        match layout {
            ExprLambdaLayout::Assignment => fits_expanded(&fmt_body).fmt(f),
            ExprLambdaLayout::Default => fmt_body.fmt(f),
        }
    }
}
