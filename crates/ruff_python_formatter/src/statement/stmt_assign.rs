use ruff_formatter::{format_args, write, FormatError, RemoveSoftLinesBuffer};
use ruff_python_ast::{
    AnyNodeRef, Expr, ExprAttribute, ExprCall, FString, Operator, StmtAssign, StringLike,
    TypeParams,
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
use crate::other::f_string::FStringLayout;
use crate::statement::trailing_semicolon;
use crate::string::implicit::{
    FormatImplicitConcatenatedStringExpanded, FormatImplicitConcatenatedStringFlat,
    ImplicitConcatenatedLayout,
};
use crate::string::StringLikeExtensions;
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
#[derive(Debug)]
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

                let string_like = StringLike::try_from(*value).ok();
                let format_f_string =
                    string_like.and_then(|string| format_f_string_assignment(string, f.context()));
                let format_implicit_flat = string_like.and_then(|string| {
                    FormatImplicitConcatenatedStringFlat::new(string, f.context())
                });

                if !can_inline_comment
                    && format_implicit_flat.is_none()
                    && format_f_string.is_none()
                {
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

                    // Special case for implicit concatenated strings in assignment value positions.
                    // The special handling is necessary to prevent an instability where an assignment has
                    // a trailing own line comment and the implicit concatenated string fits on the line,
                    // but only if the comment doesn't get inlined.
                    //
                    // ```python
                    // ____aaa = (
                    //     "aaaaaaaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvv"
                    // )  # c
                    // ```
                    //
                    // Without the special handling, this would get formatted to:
                    // ```python
                    // ____aaa = (
                    //     "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvv"
                    // )  # c
                    // ```
                    //
                    // However, this now gets reformatted again because Ruff now takes the `BestFit` layout for the string
                    // because the value is no longer an implicit concatenated string.
                    // ```python
                    // ____aaa = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvv"  # c
                    // ```
                    //
                    // The special handling here ensures that the implicit concatenated string only gets
                    // joined **if** it fits with the trailing comment inlined. Otherwise, keep the multiline
                    // formatting.
                    if let Some(flat) = format_implicit_flat {
                        inline_comments.mark_formatted();
                        let string = flat.string();

                        let flat = format_with(|f| {
                            if string.is_fstring() {
                                let mut buffer = RemoveSoftLinesBuffer::new(&mut *f);

                                write!(buffer, [flat])
                            } else {
                                flat.fmt(f)
                            }
                        })
                        .memoized();

                        // F-String containing an expression with a magic trailing comma, a comment, or a
                        // multiline debug expression should never be joined. Use the default layout.
                        // ```python
                        // aaaa = f"abcd{[
                        //    1,
                        //    2,
                        // ]}" "more"
                        // ```
                        if string.is_fstring() && flat.inspect(f)?.will_break() {
                            inline_comments.mark_unformatted();

                            return write!(
                                f,
                                [maybe_parenthesize_expression(
                                    value,
                                    *statement,
                                    Parenthesize::IfBreaks,
                                )]
                            );
                        }

                        let expanded = format_with(|f| {
                            let f =
                                &mut WithNodeLevel::new(NodeLevel::Expression(Some(group_id)), f);

                            write!(
                                f,
                                [FormatImplicitConcatenatedStringExpanded::new(
                                    string,
                                    ImplicitConcatenatedLayout::MaybeFlat
                                )]
                            )
                        });

                        // Join the implicit concatenated string if it fits on a single line
                        // ```python
                        // a = "testmorelong" # comment
                        // ```
                        let single_line = format_with(|f| write!(f, [flat, inline_comments]));

                        // Parenthesize the string but join the implicit concatenated string and inline the comment.
                        // ```python
                        // a = (
                        //      "testmorelong" # comment
                        // )
                        // ```
                        let joined_parenthesized = format_with(|f| {
                            group(&format_args![
                                token("("),
                                soft_block_indent(&format_args![flat, inline_comments]),
                                token(")"),
                            ])
                            .with_group_id(Some(group_id))
                            .should_expand(true)
                            .fmt(f)
                        });

                        // Keep the implicit concatenated string multiline and don't inline the comment.
                        // ```python
                        // a = (
                        //      "test"
                        //      "more"
                        //      "long"
                        // ) # comment
                        // ```
                        let implicit_expanded = format_with(|f| {
                            group(&format_args![
                                token("("),
                                block_indent(&expanded),
                                token(")"),
                                inline_comments,
                            ])
                            .with_group_id(Some(group_id))
                            .should_expand(true)
                            .fmt(f)
                        });

                        // We can't use `optional_parentheses` here because the `inline_comments` contains
                        // a `expand_parent` which results in an instability because the next format
                        // collapses the parentheses.
                        // We can't use `parenthesize_if_expands` because it defaults to
                        // the *flat* layout when the expanded layout doesn't fit.
                        best_fitting![single_line, joined_parenthesized, implicit_expanded]
                            .with_mode(BestFittingMode::AllLines)
                            .fmt(f)?;
                    } else if let Some(format_f_string) = format_f_string {
                        inline_comments.mark_formatted();

                        let f_string_flat = format_with(|f| {
                            let mut buffer = RemoveSoftLinesBuffer::new(&mut *f);

                            write!(buffer, [format_f_string.format()])
                        })
                        .memoized();

                        // F-String containing an expression with a magic trailing comma, a comment, or a
                        // multiline debug expression should never be joined. Use the default layout.
                        // ```python
                        // aaaa = f"aaaa {[
                        //     1, 2,
                        // ]} bbbb"
                        // ```
                        if f_string_flat.inspect(f)?.will_break() {
                            inline_comments.mark_unformatted();

                            return write!(
                                f,
                                [maybe_parenthesize_expression(
                                    value,
                                    *statement,
                                    Parenthesize::IfBreaks,
                                )]
                            );
                        }

                        // Considering the following example:
                        // ```python
                        // aaaaaaaaaaaaaaaaaa = f"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
                        //     expression}moreeeeeeeeeeeeeeeee"
                        // ```

                        // Flatten the f-string.
                        // ```python
                        // aaaaaaaaaaaaaaaaaa = f"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{expression}moreeeeeeeeeeeeeeeee"
                        // ```
                        let single_line =
                            format_with(|f| write!(f, [f_string_flat, inline_comments]));

                        // Parenthesize the f-string and flatten the f-string.
                        // ```python
                        // aaaaaaaaaaaaaaaaaa = (
                        //     f"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{expression}moreeeeeeeeeeeeeeeee"
                        // )
                        // ```
                        let joined_parenthesized = format_with(|f| {
                            group(&format_args![
                                token("("),
                                soft_block_indent(&format_args![f_string_flat, inline_comments]),
                                token(")"),
                            ])
                            .with_group_id(Some(group_id))
                            .should_expand(true)
                            .fmt(f)
                        });

                        // Avoid flattening or parenthesizing the f-string, keep the original
                        // f-string formatting.
                        // ```python
                        // aaaaaaaaaaaaaaaaaa = f"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
                        //     expression
                        // }moreeeeeeeeeeeeeeeee"
                        // ```
                        let format_f_string =
                            format_with(|f| write!(f, [format_f_string.format(), inline_comments]));

                        best_fitting![single_line, joined_parenthesized, format_f_string]
                            .with_mode(BestFittingMode::AllLines)
                            .fmt(f)?;
                    } else {
                        best_fit_parenthesize(&format_once(|f| {
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

                let string_like = StringLike::try_from(*value).ok();
                let format_f_string =
                    string_like.and_then(|string| format_f_string_assignment(string, f.context()));
                let format_implicit_flat = string_like.and_then(|string| {
                    FormatImplicitConcatenatedStringFlat::new(string, f.context())
                });

                // Use the normal `maybe_parenthesize_layout` for splittable `value`s.
                if !should_inline_comments
                    && !should_non_inlineable_use_best_fit(value, *statement, f.context())
                    && format_implicit_flat.is_none()
                    && format_f_string.is_none()
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
                let inline_comments = if should_inline_comments
                    || format_implicit_flat.is_some()
                    || format_f_string.is_some()
                {
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

                let last_target = before_operator.memoized();
                let last_target_breaks = last_target.inspect(f)?.will_break();

                // Don't parenthesize the `value` if it is known that the target will break.
                // This is mainly a performance optimisation that avoids unnecessary memoization
                // and using the costly `BestFitting` layout if it is already known that only the last variant
                // can ever fit because the left breaks.
                if format_implicit_flat.is_none() && format_f_string.is_none() && last_target_breaks
                {
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

                let format_value = format_with(|f| {
                    if let Some(format_implicit_flat) = format_implicit_flat.as_ref() {
                        if format_implicit_flat.string().is_fstring() {
                            // Remove any soft line breaks emitted by the f-string formatting.
                            // This is important when formatting f-strings as part of an assignment right side
                            // because `best_fit_parenthesize` will otherwise still try to break inner
                            // groups if wrapped in a `group(..).should_expand(true)`
                            let mut buffer = RemoveSoftLinesBuffer::new(&mut *f);
                            write!(buffer, [format_implicit_flat])
                        } else {
                            format_implicit_flat.fmt(f)
                        }
                    } else if let Some(format_f_string) = format_f_string.as_ref() {
                        // Similar to above, remove any soft line breaks emitted by the f-string
                        // formatting.
                        let mut buffer = RemoveSoftLinesBuffer::new(&mut *f);
                        write!(buffer, [format_f_string.format()])
                    } else {
                        value.format().with_options(Parentheses::Never).fmt(f)
                    }
                })
                .memoized();

                // Tries to fit the `left` and the `value` on a single line:
                // ```python
                // a = b = c
                // ```
                let single_line = format_with(|f| {
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
                let flat_target_parenthesize_value = format_with(|f| {
                    write!(
                        f,
                        [
                            last_target,
                            space(),
                            operator,
                            space(),
                            token("("),
                            group(&soft_block_indent(&format_args![
                                format_value,
                                inline_comments
                            ]))
                            .should_expand(true),
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
                let split_target_flat_value = format_with(|f| {
                    write!(
                        f,
                        [
                            group(&last_target).should_expand(true),
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
                        single_line,
                        // Avoid parenthesizing the call expression if the `(` fit on the line
                        format_args![
                            last_target,
                            space(),
                            operator,
                            space(),
                            group(&format_value).should_expand(true),
                        ],
                        flat_target_parenthesize_value,
                        split_target_flat_value
                    ]
                    .fmt(f)
                } else if let Some(format_implicit_flat) = &format_implicit_flat {
                    // F-String containing an expression with a magic trailing comma, a comment, or a
                    // multiline debug expression should never be joined. Use the default layout.
                    //
                    // ```python
                    // aaaa = f"abcd{[
                    //    1,
                    //    2,
                    // ]}" "more"
                    // ```
                    if format_implicit_flat.string().is_fstring()
                        && format_value.inspect(f)?.will_break()
                    {
                        inline_comments.mark_unformatted();

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

                    let group_id = f.group_id("optional_parentheses");
                    let format_expanded = format_with(|f| {
                        let f = &mut WithNodeLevel::new(NodeLevel::Expression(Some(group_id)), f);

                        FormatImplicitConcatenatedStringExpanded::new(
                            StringLike::try_from(*value).unwrap(),
                            ImplicitConcatenatedLayout::MaybeFlat,
                        )
                        .fmt(f)
                    })
                    .memoized();

                    // Keep the target flat, parenthesize the value, and keep it multiline.
                    //
                    // ```python
                    // Literal[ "a", "b"] = (
                    //      "looooooooooooooooooooooooooooooong"
                    //      "string"
                    //  ) # comment
                    // ```
                    let flat_target_value_parenthesized_multiline = format_with(|f| {
                        write!(
                            f,
                            [
                                last_target,
                                space(),
                                operator,
                                space(),
                                token("("),
                                group(&soft_block_indent(&format_expanded))
                                    .with_group_id(Some(group_id))
                                    .should_expand(true),
                                token(")"),
                                inline_comments
                            ]
                        )
                    });

                    // Expand the parent and parenthesize the joined string with the inlined comment.
                    //
                    // ```python
                    // Literal[
                    //     "a",
                    //     "b",
                    //  ] = (
                    //      "not that long string" # comment
                    //  )
                    // ```
                    let split_target_value_parenthesized_flat = format_with(|f| {
                        write!(
                            f,
                            [
                                group(&last_target).should_expand(true),
                                space(),
                                operator,
                                space(),
                                token("("),
                                group(&soft_block_indent(&format_args![
                                    format_value,
                                    inline_comments
                                ]))
                                .should_expand(true),
                                token(")")
                            ]
                        )
                    });

                    // The most expanded variant: Expand both the target and the string.
                    //
                    // ```python
                    // Literal[
                    //     "a",
                    //     "b",
                    //  ] = (
                    //      "looooooooooooooooooooooooooooooong"
                    //      "string"
                    //  ) # comment
                    // ```
                    let split_target_value_parenthesized_multiline = format_with(|f| {
                        write!(
                            f,
                            [
                                group(&last_target).should_expand(true),
                                space(),
                                operator,
                                space(),
                                token("("),
                                group(&soft_block_indent(&format_expanded))
                                    .with_group_id(Some(group_id))
                                    .should_expand(true),
                                token(")"),
                                inline_comments
                            ]
                        )
                    });

                    // This is only a perf optimisation. No point in trying all the "flat-target"
                    // variants if we know that the last target must break.
                    if last_target_breaks {
                        best_fitting![
                            split_target_flat_value,
                            split_target_value_parenthesized_flat,
                            split_target_value_parenthesized_multiline,
                        ]
                        .with_mode(BestFittingMode::AllLines)
                        .fmt(f)
                    } else {
                        best_fitting![
                            single_line,
                            flat_target_parenthesize_value,
                            flat_target_value_parenthesized_multiline,
                            split_target_flat_value,
                            split_target_value_parenthesized_flat,
                            split_target_value_parenthesized_multiline,
                        ]
                        .with_mode(BestFittingMode::AllLines)
                        .fmt(f)
                    }
                } else if let Some(format_f_string) = &format_f_string {
                    // F-String containing an expression with a magic trailing comma, a comment, or a
                    // multiline debug expression should never be joined. Use the default layout.
                    //
                    // ```python
                    // aaaa, bbbb = f"aaaa {[
                    //     1, 2,
                    // ]} bbbb"
                    // ```
                    if format_value.inspect(f)?.will_break() {
                        inline_comments.mark_unformatted();

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

                    let format_f_string =
                        format_with(|f| write!(f, [format_f_string.format(), inline_comments]))
                            .memoized();

                    // Considering the following initial source:
                    //
                    // ```python
                    // aaaaaaaaaaaa["bbbbbbbbbbbbbbbb"] = (
                    //     f"aaaaaaaaaaaaaaaaaaa {
                    //         aaaaaaaaa + bbbbbbbbbbb + cccccccccccccc} ddddddddddddddddddd"
                    // )
                    // ```
                    //
                    // Keep the target flat, and use the regular f-string formatting.
                    //
                    // ```python
                    // aaaaaaaaaaaa["bbbbbbbbbbbbbbbb"] = f"aaaaaaaaaaaaaaaaaaa {
                    //     aaaaaaaaa + bbbbbbbbbbb + cccccccccccccc
                    // } ddddddddddddddddddd"
                    // ```
                    let flat_target_regular_f_string = format_with(|f| {
                        write!(
                            f,
                            [last_target, space(), operator, space(), format_f_string]
                        )
                    });

                    // Expand the parent and parenthesize the flattened f-string.
                    //
                    // ```python
                    // aaaaaaaaaaaa[
                    //     "bbbbbbbbbbbbbbbb"
                    // ] = (
                    //     f"aaaaaaaaaaaaaaaaaaa {aaaaaaaaa + bbbbbbbbbbb + cccccccccccccc} ddddddddddddddddddd"
                    // )
                    // ```
                    let split_target_value_parenthesized_flat = format_with(|f| {
                        write!(
                            f,
                            [
                                group(&last_target).should_expand(true),
                                space(),
                                operator,
                                space(),
                                token("("),
                                group(&soft_block_indent(&format_args![
                                    format_value,
                                    inline_comments
                                ]))
                                .should_expand(true),
                                token(")")
                            ]
                        )
                    });

                    // Expand the parent, and use the regular f-string formatting.
                    //
                    // ```python
                    // aaaaaaaaaaaa[
                    //     "bbbbbbbbbbbbbbbb"
                    // ] = f"aaaaaaaaaaaaaaaaaaa {
                    //     aaaaaaaaa + bbbbbbbbbbb + cccccccccccccc
                    // } ddddddddddddddddddd"
                    // ```
                    let split_target_regular_f_string = format_with(|f| {
                        write!(
                            f,
                            [
                                group(&last_target).should_expand(true),
                                space(),
                                operator,
                                space(),
                                format_f_string,
                            ]
                        )
                    });

                    // This is only a perf optimisation. No point in trying all the "flat-target"
                    // variants if we know that the last target must break.
                    if last_target_breaks {
                        best_fitting![
                            split_target_flat_value,
                            split_target_value_parenthesized_flat,
                            split_target_regular_f_string,
                        ]
                        .with_mode(BestFittingMode::AllLines)
                        .fmt(f)
                    } else {
                        best_fitting![
                            single_line,
                            flat_target_parenthesize_value,
                            flat_target_regular_f_string,
                            split_target_flat_value,
                            split_target_value_parenthesized_flat,
                            split_target_regular_f_string,
                        ]
                        .with_mode(BestFittingMode::AllLines)
                        .fmt(f)
                    }
                } else {
                    best_fitting![
                        single_line,
                        flat_target_parenthesize_value,
                        split_target_flat_value
                    ]
                    .fmt(f)
                }
            }
        }
    }
}

/// Formats an f-string that is at the value position of an assignment statement.
///
/// This is just a wrapper around [`FormatFString`] while considering a special case when the
/// f-string is at an assignment statement's value position.
///
/// This is necessary to prevent an instability where an f-string contains a multiline expression
/// and the f-string fits on the line, but only when it's surrounded by parentheses.
///
/// ```python
/// aaaaaaaaaaaaaaaaaa = f"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
///     expression}moreeeeeeeeeeeeeeeee"
/// ```
///
/// Without the special handling, this would get formatted to:
/// ```python
/// aaaaaaaaaaaaaaaaaa = f"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
///     expression
/// }moreeeeeeeeeeeeeeeee"
/// ```
///
/// However, if the parentheses already existed in the source like:
/// ```python
/// aaaaaaaaaaaaaaaaaa = (
///     f"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{expression}moreeeeeeeeeeeeeeeee"
/// )
/// ```
///
/// Then, it would remain unformatted because it fits on the line. This means that even in the
/// first example, the f-string should be formatted by surrounding it with parentheses.
///
/// One might ask why not just use the `BestFit` layout in this case. Consider the following
/// example in which the f-string doesn't fit on the line even when surrounded by parentheses:
/// ```python
/// xxxxxxx = f"{
///     {'aaaaaaaaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbbbbbbb', 'cccccccccccccccccccccccccc'}
/// }"
/// ```
///
/// The `BestFit` layout will format this as:
/// ```python
/// xxxxxxx = (
///     f"{
///         {
///             'aaaaaaaaaaaaaaaaaaaaaaaaa',
///             'bbbbbbbbbbbbbbbbbbbbbbbbbbb',
///             'cccccccccccccccccccccccccc',
///         }
///     }"
/// )
/// ```
///
/// The reason for this is because (a) f-string already has a multiline expression thus it tries to
/// break the expression and (b) the `BestFit` layout doesn't considers the layout where the
/// multiline f-string isn't surrounded by parentheses.
fn format_f_string_assignment<'a>(
    string: StringLike<'a>,
    context: &PyFormatContext,
) -> Option<&'a FString> {
    let StringLike::FString(expr) = string else {
        return None;
    };

    let f_string = expr.as_single_part_fstring()?;

    // If the f-string is flat, there are no breakpoints from which it can be made multiline.
    // This is the case when the f-string has no expressions or if it does then the expressions
    // are flat (no newlines).
    if FStringLayout::from_f_string(f_string, context.source()).is_flat() {
        return None;
    }

    // This checks whether the f-string is multi-line and it can *never* be flattened. Thus,
    // it's useless to try the flattened layout.
    if string.is_multiline(context) {
        return None;
    }

    Some(f_string)
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

    fn mark_unformatted(&self) {
        for comment in self.expression {
            comment.mark_unformatted();
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
