use ruff_formatter::{format_args, write, FormatError};
use ruff_python_ast::AstNode;
use ruff_python_ast::StmtWith;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::builders::parenthesize_if_expands;
use crate::comments::SourceComment;
use crate::expression::can_omit_optional_parentheses;
use crate::expression::parentheses::{
    is_expression_parenthesized, optional_parentheses, parenthesized,
};
use crate::other::commas;
use crate::prelude::*;
use crate::preview::is_wrap_multiple_context_managers_in_parens_enabled;
use crate::statement::clause::{clause_body, clause_header, ClauseHeader};
use crate::{PyFormatOptions, PythonVersion};

#[derive(Default)]
pub struct FormatStmtWith;

impl FormatNodeRule<StmtWith> for FormatStmtWith {
    fn fmt_fields(&self, with_stmt: &StmtWith, f: &mut PyFormatter) -> FormatResult<()> {
        // The `with` statement can have one dangling comment on the open parenthesis, like:
        // ```python
        // with (  # comment
        //     CtxManager() as example
        // ):
        //     ...
        // ```
        //
        // Any other dangling comments are trailing comments on the colon, like:
        // ```python
        // with CtxManager() as example:  # comment
        //     ...
        // ```
        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling(with_stmt.as_any_node_ref());
        let partition_point = dangling_comments.partition_point(|comment| {
            with_stmt
                .items
                .first()
                .is_some_and(|with_item| with_item.start() > comment.start())
        });
        let (parenthesized_comments, colon_comments) = dangling_comments.split_at(partition_point);

        write!(
            f,
            [
                clause_header(
                    ClauseHeader::With(with_stmt),
                    colon_comments,
                    &format_with(|f| {
                        write!(
                            f,
                            [
                                with_stmt
                                    .is_async
                                    .then_some(format_args![token("async"), space()]),
                                token("with"),
                                space()
                            ]
                        )?;

                        if parenthesized_comments.is_empty() {
                            let format_items = format_with(|f| {
                                let mut joiner =
                                    f.join_comma_separated(with_stmt.body.first().unwrap().start());

                                for item in &with_stmt.items {
                                    joiner.entry_with_line_separator(
                                        item,
                                        &item.format(),
                                        soft_line_break_or_space(),
                                    );
                                }
                                joiner.finish()
                            });

                            match should_parenthesize(with_stmt, f.options(), f.context())? {
                                ParenthesizeWith::Optional => {
                                    optional_parentheses(&format_items).fmt(f)?;
                                }
                                ParenthesizeWith::IfExpands => {
                                    parenthesize_if_expands(&format_items).fmt(f)?;
                                }
                                ParenthesizeWith::UnlessCommented => {
                                    if let [item] = with_stmt.items.as_slice() {
                                        // This is similar to `maybe_parenthesize_expression`, but we're not
                                        // dealing with an expression here, it's a `WithItem`.
                                        if comments.has_leading(item) || comments.has_trailing(item)
                                        {
                                            parenthesized("(", &item.format(), ")").fmt(f)?;
                                        } else {
                                            item.format().fmt(f)?;
                                        }
                                    } else {
                                        f.join_with(format_args![token(","), space()])
                                            .entries(with_stmt.items.iter().formatted())
                                            .finish()?;
                                    }
                                }
                            }
                        } else {
                            let joined = format_with(|f: &mut PyFormatter| {
                                f.join_comma_separated(with_stmt.body.first().unwrap().start())
                                    .nodes(&with_stmt.items)
                                    .finish()
                            });

                            parenthesized("(", &joined, ")")
                                .with_dangling_comments(parenthesized_comments)
                                .fmt(f)?;
                        }

                        Ok(())
                    })
                ),
                clause_body(&with_stmt.body, colon_comments)
            ]
        )
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}

/// Determines whether the `with` items should be parenthesized (over parenthesizing each item),
/// and if so, which parenthesizing layout to use.
///
/// Parenthesize `with` items if
/// * The last item has a trailing comma (implying that the with items were parenthesized in the source)
/// * There's more than one item and they're already parenthesized
/// * There's more than one item, the [`wrap_multiple_context_managers_in_parens`](is_wrap_multiple_context_managers_in_parens) preview style is enabled,
///     and the target python version is >= 3.9
/// * There's a single non-parenthesized item. The function returns [`ParenthesizeWith::Optional`]
///     if the parentheses can be omitted if breaking around parenthesized sub-expressions is sufficient
///     to make the expression fit. It returns [`ParenthesizeWith::IfExpands`] otherwise.
/// * The only item is parenthesized and has comments.
fn should_parenthesize(
    with: &StmtWith,
    options: &PyFormatOptions,
    context: &PyFormatContext,
) -> FormatResult<ParenthesizeWith> {
    if has_magic_trailing_comma(with, options, context) {
        return Ok(ParenthesizeWith::IfExpands);
    }

    let can_parenthesize = (is_wrap_multiple_context_managers_in_parens_enabled(context)
        && options.target_version() >= PythonVersion::Py39)
        || are_with_items_parenthesized(with, context)?;

    if !can_parenthesize {
        return Ok(ParenthesizeWith::UnlessCommented);
    }

    if let [single] = with.items.as_slice() {
        return Ok(
            // If the with item itself has comments (not the context expression), then keep the parentheses
            if context.comments().has_leading(single) || context.comments().has_trailing(single) {
                ParenthesizeWith::IfExpands
            }
            // If it is the only expression and it has comments, then the with statement
            // as well as the with item add parentheses
            else if is_expression_parenthesized(
                (&single.context_expr).into(),
                context.comments().ranges(),
                context.source(),
            ) {
                // Preserve the parentheses around the context expression instead of parenthesizing the entire
                // with items.
                ParenthesizeWith::UnlessCommented
            } else if is_wrap_multiple_context_managers_in_parens_enabled(context)
                && can_omit_optional_parentheses(&single.context_expr, context)
            {
                ParenthesizeWith::Optional
            } else {
                ParenthesizeWith::IfExpands
            },
        );
    }

    // Always parenthesize multiple items
    Ok(ParenthesizeWith::IfExpands)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParenthesizeWith {
    /// Don't wrap the with items in parentheses except if it is a single item
    /// and it has leading or trailing comment.
    ///
    /// This is required because `are_with_items_parenthesized` cannot determine if
    /// `with (expr)` is a parenthesized expression or a parenthesized with item.
    UnlessCommented,

    /// Wrap the with items in optional parentheses
    Optional,

    /// Wrap the with items in parentheses if they expand
    IfExpands,
}

fn has_magic_trailing_comma(
    with: &StmtWith,
    options: &PyFormatOptions,
    context: &PyFormatContext,
) -> bool {
    let Some(last_item) = with.items.last() else {
        return false;
    };

    commas::has_magic_trailing_comma(
        TextRange::new(last_item.end(), with.end()),
        options,
        context,
    )
}

fn are_with_items_parenthesized(with: &StmtWith, context: &PyFormatContext) -> FormatResult<bool> {
    let [first_item, _, ..] = with.items.as_slice() else {
        return Ok(false);
    };

    let before_first_item = TextRange::new(with.start(), first_item.start());

    let mut tokenizer = SimpleTokenizer::new(context.source(), before_first_item)
        .skip_trivia()
        .skip_while(|t| t.kind() == SimpleTokenKind::Async);

    let with_keyword = tokenizer.next().ok_or(FormatError::syntax_error(
        "Expected a with keyword, didn't find any token",
    ))?;

    debug_assert_eq!(
        with_keyword.kind(),
        SimpleTokenKind::With,
        "Expected with keyword but at {with_keyword:?}"
    );

    match tokenizer.next() {
        Some(left_paren) => {
            debug_assert_eq!(left_paren.kind(), SimpleTokenKind::LParen);
            Ok(true)
        }
        None => Ok(false),
    }
}
