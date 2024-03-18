use ruff_formatter::{format_args, write, FormatError};
use ruff_python_ast::{
    AnyNodeRef, Expr, ExprAttribute, ExprCall, Operator, StmtAssign, TypeParams,
};

use crate::builders::parenthesize_if_expands;
use crate::comments::{
    trailing_comments, Comments, LeadingDanglingTrailingComments, SourceComment,
};
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::parentheses::{
    is_expression_parenthesized, optional_parentheses, NeedsParentheses, OptionalParentheses,
    Parentheses, Parenthesize,
};
use crate::expression::{
    can_omit_optional_parentheses, has_own_parentheses, has_parentheses,
    maybe_parenthesize_expression,
};
use crate::statement::trailing_semicolon;
use crate::{has_skip_comment, prelude::*};

#[derive(Default)]
pub struct FormatStmtAssign;

impl FormatNodeRule<StmtAssign> for FormatStmtAssign {
    fn fmt_fields(&self, item: &StmtAssign, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAssign {
            range: _,
            targets,
            value,
        } = item;

        let (first, rest) = targets.split_first().ok_or(FormatError::syntax_error(
            "Expected at least on assignment target",
        ))?;

        // The first target is special because it never gets parenthesized nor does the formatter remove parentheses if unnecessary.
        let format_first = FormatTargetWithEqualOperator {
            target: first,
            preserve_parentheses: true,
        };

        // Avoid parenthesizing the value if the last target before the assigned value expands.
        if let Some((last, head)) = rest.split_last() {
            format_first.fmt(f)?;

            for target in head {
                FormatTargetWithEqualOperator {
                    target,
                    preserve_parentheses: false,
                }
                .fmt(f)?;
            }

            FormatStatementsLastExpression::RightToLeft {
                before_operator: AnyBeforeOperator::Expression(last),
                operator: AnyAssignmentOperator::Assign,
                value,
                statement: item.into(),
            }
            .fmt(f)?;
        }
        // Avoid parenthesizing the value for single-target assignments where the
        // target has its own parentheses (list, dict, tuple, ...) and the target expands.
        else if has_target_own_parentheses(first, f.context())
            && !is_expression_parenthesized(
                first.into(),
                f.context().comments().ranges(),
                f.context().source(),
            )
        {
            FormatStatementsLastExpression::RightToLeft {
                before_operator: AnyBeforeOperator::Expression(first),
                operator: AnyAssignmentOperator::Assign,
                value,
                statement: item.into(),
            }
            .fmt(f)?;
        }
        // For single targets that have no split points, parenthesize the value only
        // if it makes it fit. Otherwise omit the parentheses.
        else {
            format_first.fmt(f)?;
            FormatStatementsLastExpression::left_to_right(value, item).fmt(f)?;
        }

        if f.options().source_type().is_ipynb()
            && f.context().node_level().is_last_top_level_statement()
            && trailing_semicolon(item.into(), f.context().source()).is_some()
            && matches!(targets.as_slice(), [Expr::Name(_)])
        {
            token(";").fmt(f)?;
        }

        Ok(())
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        has_skip_comment(trailing_comments, context.source())
    }
}

/// Formats a single target with the equal operator.
struct FormatTargetWithEqualOperator<'a> {
    target: &'a Expr,

    /// Whether parentheses should be preserved as in the source or if the target
    /// should only be parenthesized if necessary (because of comments or because it doesn't fit).
    preserve_parentheses: bool,
}

impl Format<PyFormatContext<'_>> for FormatTargetWithEqualOperator<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        // Preserve parentheses for the first target or around targets with leading or trailing comments.
        if self.preserve_parentheses
            || f.context().comments().has_leading(self.target)
            || f.context().comments().has_trailing(self.target)
        {
            self.target.format().fmt(f)?;
        } else if should_parenthesize_target(self.target, f.context()) {
            parenthesize_if_expands(&self.target.format().with_options(Parentheses::Never))
                .fmt(f)?;
        } else {
            self.target
                .format()
                .with_options(Parentheses::Never)
                .fmt(f)?;
        }

        write!(f, [space(), token("="), space()])
    }
}

/// Formats the last expression in statements that start with a keyword (like `return`) or after an operator (assignments).
///
/// The implementation avoids parenthesizing unsplittable values (like `None`, `True`, `False`, Names, a subset of strings)
/// if the value won't fit even when parenthesized.
///
/// ## Trailing comments
/// Trailing comments are inlined inside the `value`'s parentheses rather than formatted at the end
/// of the statement for unsplittable values if the `value` gets parenthesized.
///
/// Inlining the trailing comments prevent situations where the parenthesized value
/// still exceeds the configured line width, but parenthesizing helps to make the trailing comment fit.
/// Instead, it only parenthesizes `value` if it makes both the `value` and the trailing comment fit.
/// See [PR 8431](https://github.com/astral-sh/ruff/pull/8431) for more details.
///
/// The implementation formats the statement's and value's trailing end of line comments:
/// * after the expression if the expression needs no parentheses (necessary or the `expand_parent` makes the group never fit).
/// * inside the parentheses if the expression exceeds the line-width.
///
/// ```python
/// a = loooooooooooooooooooooooooooong # with_comment
/// b = (
///     short # with_comment
/// )
/// ```
///
/// Which gets formatted to:
///
/// ```python
/// # formatted
/// a = (
///     loooooooooooooooooooooooooooong # with comment
/// )
/// b = short # with comment
/// ```
///
/// The long name gets parenthesized because it exceeds the configured line width and the trailing comment of the
/// statement gets formatted inside (instead of outside) the parentheses.
///
/// No parentheses are added for `short` because it fits into the configured line length, regardless of whether
/// the comment exceeds the line width or not.
///
/// This logic isn't implemented in [`place_comment`] by associating trailing statement comments to the expression because
/// doing so breaks the suite empty lines formatting that relies on trailing comments to be stored on the statement.
pub(super) enum FormatStatementsLastExpression<'a> {
    /// Prefers to split what's left of `value` before splitting the value.
    ///
    /// ```python
    /// aaaaaaa[bbbbbbbb] = some_long_value
    /// ```
    ///
    /// This layout splits `aaaaaaa[bbbbbbbb]` first assuming the whole statements exceeds the line width, resulting in
    ///
    /// ```python
    /// aaaaaaa[
    ///     bbbbbbbb
    /// ] = some_long_value
    /// ```
    ///
    /// This layout is preferred over [`RightToLeft`] if the left is unsplittable (single keyword like `return` or a Name)
    /// because it has better performance characteristics.
    LeftToRight {
        /// The right side of an assignment or the value returned in a return statement.
        value: &'a Expr,

        /// The parent statement that encloses the `value` expression.
        statement: AnyNodeRef<'a>,
    },

    /// Prefers parenthesizing the value before splitting the left side. Specific to assignments.
    ///
    /// Formats what's left of `value` together with the assignment operator and the assigned `value`.
    /// This layout prefers parenthesizing the value over parenthesizing the left (target or type annotation):
    ///
    /// ```python
    /// aaaaaaa[bbbbbbbb] = some_long_value
    /// ```
    ///
    /// gets formatted to...
    ///
    /// ```python
    /// aaaaaaa[bbbbbbbb] = (
    ///     some_long_value
    /// )
    /// ```
    ///
    /// ... regardless whether the value will fit or not.
    ///
    /// The left only gets parenthesized if the left exceeds the configured line width on its own or
    /// is forced to split because of a magical trailing comma or contains comments:
    ///
    /// ```python
    /// aaaaaaa[bbbbbbbb_exceeds_the_line_width] = some_long_value
    /// ```
    ///
    /// gets formatted to
    /// ```python
    /// aaaaaaa[
    ///     bbbbbbbb_exceeds_the_line_width
    /// ] = some_long_value
    /// ```
    ///
    /// The layout avoids parenthesizing the value when the left splits to avoid
    /// unnecessary parentheses. Adding the parentheses, as shown in the below example, reduces readability.
    ///
    /// ```python
    /// aaaaaaa[
    ///     bbbbbbbb_exceeds_the_line_width
    /// ] = (
    ///     some_long_value
    /// )
    ///
    /// ## Non-fluent Call Expressions
    /// Non-fluent call expressions in the `value` position are only parenthesized if the opening parentheses
    /// exceeds the configured line length. The layout prefers splitting after the opening parentheses
    /// if the `callee` expression and the opening parentheses fit.
    /// fits on the line.
    RightToLeft {
        /// The expression that comes before the assignment operator. This is either
        /// the last target, or the type annotation of an annotated assignment.
        before_operator: AnyBeforeOperator<'a>,

        /// The assignment operator. Either `Assign` (`=`) or the operator used by the augmented assignment statement.
        operator: AnyAssignmentOperator,

        /// The assigned `value`.
        value: &'a Expr,

        /// The assignment statement.
        statement: AnyNodeRef<'a>,
    },
}

impl<'a> FormatStatementsLastExpression<'a> {
    pub(super) fn left_to_right<S: Into<AnyNodeRef<'a>>>(value: &'a Expr, statement: S) -> Self {
        Self::LeftToRight {
            value,
            statement: statement.into(),
        }
    }
}

impl Format<PyFormatContext<'_>> for FormatStatementsLastExpression<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        match self {
            FormatStatementsLastExpression::LeftToRight { value, statement } => {
                let can_inline_comment = should_inline_comments(value, *statement, f.context());

                if !can_inline_comment {
                    return maybe_parenthesize_expression(
                        value,
                        *statement,
                        Parenthesize::IfBreaks,
                    )
                    .fmt(f);
                }

                let comments = f.context().comments().clone();
                let expression_comments = comments.leading_dangling_trailing(*value);

                if let Some(inline_comments) = OptionalParenthesesInlinedComments::new(
                    &expression_comments,
                    *statement,
                    &comments,
                ) {
                    let group_id = f.group_id("optional_parentheses");

                    let f = &mut WithNodeLevel::new(NodeLevel::Expression(Some(group_id)), f);

                    best_fit_parenthesize(&format_with(|f| {
                        inline_comments.mark_formatted();

                        value.format().with_options(Parentheses::Never).fmt(f)?;

                        if !inline_comments.is_empty() {
                            // If the expressions exceeds the line width, format the comments in the parentheses
                            if_group_breaks(&inline_comments).fmt(f)?;
                        }

                        Ok(())
                    }))
                    .with_group_id(Some(group_id))
                    .fmt(f)?;

                    if !inline_comments.is_empty() {
                        // If the line fits into the line width, format the comments after the parenthesized expression
                        if_group_fits_on_line(&inline_comments)
                            .with_group_id(Some(group_id))
                            .fmt(f)?;
                    }

                    Ok(())
                } else {
                    // Preserve the parentheses if the expression has any leading or trailing comments,
                    // to avoid syntax errors, similar to `maybe_parenthesize_expression`.
                    value.format().with_options(Parentheses::Always).fmt(f)
                }
            }
            FormatStatementsLastExpression::RightToLeft {
                before_operator,
                operator,
                value,
                statement,
            } => {
                let should_inline_comments = should_inline_comments(value, *statement, f.context());

                // Use the normal `maybe_parenthesize_layout` for splittable `value`s.
                if !should_inline_comments
                    && !should_non_inlineable_use_best_fit(value, *statement, f.context())
                {
                    return write!(
                        f,
                        [
                            before_operator,
                            space(),
                            operator,
                            space(),
                            maybe_parenthesize_expression(
                                value,
                                *statement,
                                Parenthesize::IfBreaks
                            )
                        ]
                    );
                }

                let comments = f.context().comments().clone();
                let expression_comments = comments.leading_dangling_trailing(*value);

                // Don't inline comments for attribute and call expressions for black compatibility
                let inline_comments = if should_inline_comments {
                    OptionalParenthesesInlinedComments::new(
                        &expression_comments,
                        *statement,
                        &comments,
                    )
                } else if expression_comments.has_leading()
                    || expression_comments.has_trailing_own_line()
                {
                    None
                } else {
                    Some(OptionalParenthesesInlinedComments::default())
                };

                let Some(inline_comments) = inline_comments else {
                    // Preserve the parentheses if the expression has any leading or trailing own line comments
                    // same as `maybe_parenthesize_expression`
                    return write!(
                        f,
                        [
                            before_operator,
                            space(),
                            operator,
                            space(),
                            value.format().with_options(Parentheses::Always)
                        ]
                    );
                };

                // Prevent inline comments to be formatted as part of the expression.
                inline_comments.mark_formatted();

                let mut last_target = before_operator.memoized();

                // Don't parenthesize the `value` if it is known that the target will break.
                // This is mainly a performance optimisation that avoids unnecessary memoization
                // and using the costly `BestFitting` layout if it is already known that only the last variant
                // can ever fit because the left breaks.
                if last_target.inspect(f)?.will_break() {
                    return write!(
                        f,
                        [
                            last_target,
                            space(),
                            operator,
                            space(),
                            value.format().with_options(Parentheses::Never),
                            inline_comments
                        ]
                    );
                }

                let format_value = value.format().with_options(Parentheses::Never).memoized();

                // Tries to fit the `left` and the `value` on a single line:
                // ```python
                // a = b = c
                // ```
                let format_flat = format_with(|f| {
                    write!(
                        f,
                        [
                            last_target,
                            space(),
                            operator,
                            space(),
                            format_value,
                            inline_comments
                        ]
                    )
                });

                // Don't break the last assignment target but parenthesize the value to see if it fits (break right first).
                //
                // ```python
                // a["bbbbb"] = (
                //      c
                // )
                // ```
                let format_parenthesize_value = format_with(|f| {
                    write!(
                        f,
                        [
                            last_target,
                            space(),
                            operator,
                            space(),
                            token("("),
                            block_indent(&format_args![format_value, inline_comments]),
                            token(")")
                        ]
                    )
                });

                // Fall back to parenthesizing (or splitting) the last target part if we can't make the value
                // fit. Don't parenthesize the value to avoid unnecessary parentheses.
                //
                // ```python
                // a[
                //      "bbbbb"
                // ] = c
                // ```
                let format_split_left = format_with(|f| {
                    write!(
                        f,
                        [
                            last_target,
                            space(),
                            operator,
                            space(),
                            format_value,
                            inline_comments
                        ]
                    )
                });

                // For call expressions, prefer breaking after the call expression's opening parentheses
                // over parenthesizing the entire call expression.
                // For subscripts, try breaking the subscript first
                // For attribute chains that contain any parenthesized value: Try expanding the parenthesized value first.
                if value.is_call_expr() || value.is_subscript_expr() || value.is_attribute_expr() {
                    best_fitting![
                        format_flat,
                        // Avoid parenthesizing the call expression if the `(` fit on the line
                        format_args![
                            last_target,
                            space(),
                            operator,
                            space(),
                            group(&format_value).should_expand(true),
                        ],
                        format_parenthesize_value,
                        format_split_left
                    ]
                    .fmt(f)
                } else {
                    best_fitting![format_flat, format_parenthesize_value, format_split_left].fmt(f)
                }
            }
        }
    }
}

#[derive(Debug, Default)]
struct OptionalParenthesesInlinedComments<'a> {
    expression: &'a [SourceComment],
    statement: &'a [SourceComment],
}

impl<'a> OptionalParenthesesInlinedComments<'a> {
    fn new(
        expression_comments: &LeadingDanglingTrailingComments<'a>,
        statement: AnyNodeRef<'a>,
        comments: &'a Comments<'a>,
    ) -> Option<Self> {
        if expression_comments.has_leading() || expression_comments.has_trailing_own_line() {
            return None;
        }

        let statement_trailing_comments = comments.trailing(statement);
        let after_end_of_line = statement_trailing_comments
            .partition_point(|comment| comment.line_position().is_end_of_line());
        let (stmt_inline_comments, _) = statement_trailing_comments.split_at(after_end_of_line);

        let after_end_of_line = expression_comments
            .trailing
            .partition_point(|comment| comment.line_position().is_end_of_line());

        let (expression_inline_comments, trailing_own_line_comments) =
            expression_comments.trailing.split_at(after_end_of_line);

        debug_assert!(trailing_own_line_comments.is_empty(), "The method should have returned early if the expression has trailing own line comments");

        Some(OptionalParenthesesInlinedComments {
            expression: expression_inline_comments,
            statement: stmt_inline_comments,
        })
    }

    fn is_empty(&self) -> bool {
        self.expression.is_empty() && self.statement.is_empty()
    }

    fn iter_comments(&self) -> impl Iterator<Item = &'a SourceComment> {
        self.expression.iter().chain(self.statement)
    }

    fn mark_formatted(&self) {
        for comment in self.expression {
            comment.mark_formatted();
        }
    }
}

impl Format<PyFormatContext<'_>> for OptionalParenthesesInlinedComments<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        for comment in self.iter_comments() {
            comment.mark_unformatted();
        }

        write!(
            f,
            [
                trailing_comments(self.expression),
                trailing_comments(self.statement)
            ]
        )
    }
}

#[derive(Copy, Clone, Debug)]
pub(super) enum AnyAssignmentOperator {
    Assign,
    AugAssign(Operator),
}

impl Format<PyFormatContext<'_>> for AnyAssignmentOperator {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        match self {
            AnyAssignmentOperator::Assign => token("=").fmt(f),
            AnyAssignmentOperator::AugAssign(operator) => {
                write!(f, [operator.format(), token("=")])
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub(super) enum AnyBeforeOperator<'a> {
    Expression(&'a Expr),
    TypeParams(&'a TypeParams),
}

impl Format<PyFormatContext<'_>> for AnyBeforeOperator<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        match self {
            AnyBeforeOperator::Expression(expression) => {
                // Preserve parentheses around targets with comments.
                if f.context().comments().has_leading(*expression)
                    || f.context().comments().has_trailing(*expression)
                {
                    expression
                        .format()
                        .with_options(Parentheses::Preserve)
                        .fmt(f)
                }
                // Never parenthesize targets that come with their own parentheses, e.g. don't parenthesize lists or dictionary literals.
                else if should_parenthesize_target(expression, f.context()) {
                    if can_omit_optional_parentheses(expression, f.context()) {
                        optional_parentheses(&expression.format().with_options(Parentheses::Never))
                            .fmt(f)
                    } else {
                        parenthesize_if_expands(
                            &expression.format().with_options(Parentheses::Never),
                        )
                        .fmt(f)
                    }
                } else {
                    expression.format().with_options(Parentheses::Never).fmt(f)
                }
            }
            // Never parenthesize type params
            AnyBeforeOperator::TypeParams(type_params) => type_params.format().fmt(f),
        }
    }
}

/// Returns `true` for unsplittable expressions for which comments should be inlined.
fn should_inline_comments(
    expression: &Expr,
    parent: AnyNodeRef,
    context: &PyFormatContext,
) -> bool {
    match expression {
        Expr::Name(_) | Expr::NoneLiteral(_) | Expr::NumberLiteral(_) | Expr::BooleanLiteral(_) => {
            true
        }
        Expr::StringLiteral(string) => {
            string.needs_parentheses(parent, context) == OptionalParentheses::BestFit
        }
        Expr::BytesLiteral(bytes) => {
            bytes.needs_parentheses(parent, context) == OptionalParentheses::BestFit
        }
        Expr::FString(fstring) => {
            fstring.needs_parentheses(parent, context) == OptionalParentheses::BestFit
        }
        _ => false,
    }
}

/// Tests whether an expression for which comments shouldn't be inlined should use the best fit layout
fn should_non_inlineable_use_best_fit(
    expr: &Expr,
    parent: AnyNodeRef,
    context: &PyFormatContext,
) -> bool {
    match expr {
        Expr::Attribute(attribute) => {
            attribute.needs_parentheses(parent, context) == OptionalParentheses::BestFit
        }
        Expr::Call(call) => call.needs_parentheses(parent, context) == OptionalParentheses::BestFit,
        Expr::Subscript(subscript) => {
            subscript.needs_parentheses(parent, context) == OptionalParentheses::BestFit
        }
        _ => false,
    }
}

/// Returns `true` for targets that have their own set of parentheses when they split,
/// in which case we want to avoid parenthesizing the assigned value.
pub(super) fn has_target_own_parentheses(target: &Expr, context: &PyFormatContext) -> bool {
    matches!(target, Expr::Tuple(_)) || has_own_parentheses(target, context).is_some()
}

pub(super) fn should_parenthesize_target(target: &Expr, context: &PyFormatContext) -> bool {
    !(has_target_own_parentheses(target, context)
        || is_attribute_with_parenthesized_value(target, context))
}

fn is_attribute_with_parenthesized_value(target: &Expr, context: &PyFormatContext) -> bool {
    match target {
        Expr::Attribute(ExprAttribute { value, .. }) => {
            has_parentheses(value.as_ref(), context).is_some()
                || is_attribute_with_parenthesized_value(value, context)
        }
        Expr::Subscript(_) => true,
        Expr::Call(ExprCall { arguments, .. }) => !arguments.is_empty(),
        _ => false,
    }
}
