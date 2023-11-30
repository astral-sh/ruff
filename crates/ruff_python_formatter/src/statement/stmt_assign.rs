use ruff_formatter::{format_args, write, FormatError};
use ruff_python_ast::{AnyNodeRef, Expr, StmtAssign};

use crate::comments::{trailing_comments, SourceComment, SuppressionKind};
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, Parentheses, Parenthesize,
};
use crate::expression::{has_own_parentheses, maybe_parenthesize_expression};
use crate::prelude::*;
use crate::statement::trailing_semicolon;

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

        write!(
            f,
            [
                first.format(),
                space(),
                token("="),
                space(),
                FormatTargets { targets: rest }
            ]
        )?;

        FormatStatementsLastExpression::new(value, item).fmt(f)?;

        if f.options().source_type().is_ipynb()
            && f.context().node_level().is_last_top_level_statement()
            && rest.is_empty()
            && first.is_name_expr()
            && trailing_semicolon(item.into(), f.context().source()).is_some()
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
        SuppressionKind::has_skip_comment(trailing_comments, context.source())
    }
}

#[derive(Debug)]
struct FormatTargets<'a> {
    targets: &'a [Expr],
}

impl Format<PyFormatContext<'_>> for FormatTargets<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        if let Some((first, rest)) = self.targets.split_first() {
            let comments = f.context().comments();

            let parenthesize = if comments.has_leading(first) {
                ParenthesizeTarget::Always
            } else if has_own_parentheses(first, f.context()).is_some() {
                ParenthesizeTarget::Never
            } else {
                ParenthesizeTarget::IfBreaks
            };

            let group_id = if parenthesize == ParenthesizeTarget::Never {
                Some(f.group_id("assignment_parentheses"))
            } else {
                None
            };

            let format_first = format_with(|f: &mut PyFormatter| {
                let mut f = WithNodeLevel::new(NodeLevel::Expression(group_id), f);
                match parenthesize {
                    ParenthesizeTarget::Always => {
                        write!(f, [first.format().with_options(Parentheses::Always)])
                    }
                    ParenthesizeTarget::Never => {
                        write!(f, [first.format().with_options(Parentheses::Never)])
                    }
                    ParenthesizeTarget::IfBreaks => {
                        write!(
                            f,
                            [
                                if_group_breaks(&token("(")),
                                soft_block_indent(&first.format().with_options(Parentheses::Never)),
                                if_group_breaks(&token(")"))
                            ]
                        )
                    }
                }
            });

            write!(
                f,
                [group(&format_args![
                    format_first,
                    space(),
                    token("="),
                    space(),
                    FormatTargets { targets: rest }
                ])
                .with_group_id(group_id)]
            )
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParenthesizeTarget {
    Always,
    Never,
    IfBreaks,
}

/// Formats the last expression in statements that start with a keyword (like `return`) or after an operator (assignments).
///
/// It avoids parenthesizing unsplittable values (like `None`, `True`, `False`, Names, a subset of strings) just to make
/// the trailing comment fit and inlines a trailing comment if the value itself exceeds the configured line width:
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
/// The long name gets parenthesized because it exceeds the configured line width and the trailing comma of the
/// statement gets formatted inside (instead of outside) the parentheses.
///
/// The `short` name gets unparenthesized because it fits into the configured line length, regardless of whether
/// the comment exceeds the line width or not.
///
/// This logic isn't implemented in [`place_comment`] by associating trailing statement comments to the expression because
/// doing so breaks the suite empty lines formatting that relies on trailing comments to be stored on the statement.
pub(super) struct FormatStatementsLastExpression<'a> {
    expression: &'a Expr,
    parent: AnyNodeRef<'a>,
}

impl<'a> FormatStatementsLastExpression<'a> {
    pub(super) fn new<P: Into<AnyNodeRef<'a>>>(expression: &'a Expr, parent: P) -> Self {
        Self {
            expression,
            parent: parent.into(),
        }
    }
}

impl Format<PyFormatContext<'_>> for FormatStatementsLastExpression<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let can_inline_comment = match self.expression {
            Expr::Name(_)
            | Expr::NoneLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_) => true,
            Expr::StringLiteral(string) => {
                string.needs_parentheses(self.parent, f.context()) == OptionalParentheses::BestFit
            }
            Expr::BytesLiteral(bytes) => {
                bytes.needs_parentheses(self.parent, f.context()) == OptionalParentheses::BestFit
            }
            Expr::FString(fstring) => {
                fstring.needs_parentheses(self.parent, f.context()) == OptionalParentheses::BestFit
            }
            _ => false,
        };

        if !can_inline_comment {
            return maybe_parenthesize_expression(
                self.expression,
                self.parent,
                Parenthesize::IfBreaks,
            )
            .fmt(f);
        }

        let comments = f.context().comments().clone();
        let expression_comments = comments.leading_dangling_trailing(self.expression);

        if expression_comments.has_leading() {
            // Preserve the parentheses if the expression has any leading comments,
            // same as `maybe_parenthesize_expression`
            return self
                .expression
                .format()
                .with_options(Parentheses::Always)
                .fmt(f);
        }

        let statement_trailing_comments = comments.trailing(self.parent);
        let after_end_of_line = statement_trailing_comments
            .partition_point(|comment| comment.line_position().is_end_of_line());
        let (stmt_inline_comments, _) = statement_trailing_comments.split_at(after_end_of_line);

        let after_end_of_line = expression_comments
            .trailing
            .partition_point(|comment| comment.line_position().is_end_of_line());

        let (expression_inline_comments, expression_trailing_comments) =
            expression_comments.trailing.split_at(after_end_of_line);

        if expression_trailing_comments.is_empty() {
            let inline_comments = OptionalParenthesesInlinedComments {
                expression: expression_inline_comments,
                statement: stmt_inline_comments,
            };

            let group_id = f.group_id("optional_parentheses");
            let f = &mut WithNodeLevel::new(NodeLevel::Expression(Some(group_id)), f);

            best_fit_parenthesize(&format_with(|f| {
                inline_comments.mark_formatted();

                self.expression
                    .format()
                    .with_options(Parentheses::Never)
                    .fmt(f)?;

                if !inline_comments.is_empty() {
                    // If the expressions exceeds the line width, format the comments in the parentheses
                    if_group_breaks(&inline_comments)
                        .with_group_id(Some(group_id))
                        .fmt(f)?;
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
            self.expression
                .format()
                .with_options(Parentheses::Always)
                .fmt(f)
        }
    }
}

#[derive(Debug, Default)]
struct OptionalParenthesesInlinedComments<'a> {
    expression: &'a [SourceComment],
    statement: &'a [SourceComment],
}

impl<'a> OptionalParenthesesInlinedComments<'a> {
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
