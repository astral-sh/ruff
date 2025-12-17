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

        // Format any dangling comments before the parameters, but save any dangling comments after
        // the parameters/after the header to be formatted with the body below.
        let dangling_header_comments = if let Some(parameters) = parameters {
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
            if preview && !comments.contains_comments(parameters.into()) {
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

            dangling_after_parameters
        } else {
            dangling
        };

        write!(f, [token(":")])?;

        if dangling_header_comments.is_empty() {
            write!(f, [space()])?;
        } else if !preview {
            write!(f, [dangling_comments(dangling_header_comments)])?;
        }

        if !preview {
            return body.format().fmt(f);
        }

        let fmt_body = FormatBody {
            body,
            dangling_header_comments,
        };

        match self.layout {
            ExprLambdaLayout::Assignment => fits_expanded(&fmt_body).fmt(f),
            ExprLambdaLayout::Default => fmt_body.fmt(f),
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

struct FormatBody<'a> {
    body: &'a Expr,

    /// Dangling comments attached to the lambda header that should be formatted with the body.
    ///
    /// These can include both own-line and end-of-line comments. For lambdas with parameters, this
    /// means comments after the parameters:
    ///
    /// ```py
    /// (
    ///     lambda x, y  # 1
    ///         # 2
    ///         :  # 3
    ///         # 4
    ///         x + y
    /// )
    /// ```
    ///
    /// Or all dangling comments for lambdas without parameters:
    ///
    /// ```py
    /// (
    ///     lambda  # 1
    ///         # 2
    ///         :  # 3
    ///         # 4
    ///         1
    /// )
    /// ```
    ///
    /// In most cases these should formatted within the parenthesized body, as in:
    ///
    /// ```py
    /// (
    ///     lambda: (  # 1
    ///         # 2
    ///         # 3
    ///         # 4
    ///         1
    ///     )
    /// )
    /// ```
    ///
    /// or without `# 2`:
    ///
    /// ```py
    /// (
    ///     lambda: (  # 1  # 3
    ///         # 4
    ///         1
    ///     )
    /// )
    /// ```
    dangling_header_comments: &'a [SourceComment],
}

impl Format<PyFormatContext<'_>> for FormatBody<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let FormatBody {
            dangling_header_comments,
            body,
        } = self;

        let body = *body;
        let comments = f.context().comments().clone();
        let body_comments = comments.leading_dangling_trailing(body);

        if !dangling_header_comments.is_empty() {
            // Split the dangling header comments into trailing comments formatted with the lambda
            // header (1) and leading comments formatted with the body (2, 3, 4).
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
            // Note that these are split based on their line position rather than using
            // `partition_point` based on a range, for example.
            let (trailing_header_comments, leading_body_comments) = dangling_header_comments
                .split_at(
                    dangling_header_comments
                        .iter()
                        .position(|comment| comment.line_position().is_own_line())
                        .unwrap_or(dangling_header_comments.len()),
                );

            // If the body is parenthesized and has its own leading comments, preserve the
            // separation between the dangling lambda comments and the body comments. For
            // example, preserve this comment positioning:
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
            // 1 and 2 are dangling on the lambda and emitted first, followed by a hard line
            // break and the parenthesized body with its leading comments.
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
            if is_expression_parenthesized(body.into(), comments.ranges(), f.context().source())
                && comments.has_leading(body)
            {
                trailing_comments(dangling_header_comments).fmt(f)?;

                // Note that `leading_body_comments` have already been formatted as part of
                // `dangling_header_comments` above, but their presence still determines the spacing
                // here.
                if leading_body_comments.is_empty() {
                    space().fmt(f)?;
                } else {
                    hard_line_break().fmt(f)?;
                }

                body.format().with_options(Parentheses::Always).fmt(f)
            } else {
                write!(
                    f,
                    [
                        space(),
                        token("("),
                        trailing_comments(trailing_header_comments),
                        block_indent(&format_args!(
                            leading_comments(leading_body_comments),
                            body.format().with_options(Parentheses::Never)
                        )),
                        token(")")
                    ]
                )
            }
        }
        // If the body has comments, we always want to preserve the parentheses. This also
        // ensures that we correctly handle parenthesized comments, and don't need to worry
        // about them in the implementation below.
        else if body_comments.has_leading() || body_comments.has_trailing_own_line() {
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
            )
            .is_fluent()
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
    }
}
