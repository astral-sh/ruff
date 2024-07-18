use ruff_formatter::{format_args, write, FormatContext, FormatError};
use ruff_python_ast::StmtWith;
use ruff_python_ast::{AstNode, WithItem};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::builders::parenthesize_if_expands;
use crate::comments::SourceComment;
use crate::expression::can_omit_optional_parentheses;
use crate::expression::parentheses::{
    is_expression_parenthesized, optional_parentheses, parenthesized,
};
use crate::other::commas;
use crate::other::with_item::WithItemLayout;
use crate::prelude::*;
use crate::statement::clause::{clause_body, clause_header, ClauseHeader};
use crate::statement::suite::SuiteKind;
use crate::PythonVersion;

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

                        let layout = WithItemsLayout::from_statement(
                            with_stmt,
                            f.context(),
                            parenthesized_comments,
                        )?;

                        match layout {
                            WithItemsLayout::SingleWithTarget(single) => {
                                optional_parentheses(&single.format()).fmt(f)
                            }

                            WithItemsLayout::SingleWithoutTarget(single) => single
                                .format()
                                .with_options(WithItemLayout::SingleWithoutTarget)
                                .fmt(f),

                            WithItemsLayout::SingleParenthesizedContextManager(single) => single
                                .format()
                                .with_options(WithItemLayout::SingleParenthesizedContextManager)
                                .fmt(f),

                            WithItemsLayout::ParenthesizeIfExpands => {
                                parenthesize_if_expands(&format_with(|f| {
                                    let mut joiner = f.join_comma_separated(
                                        with_stmt.body.first().unwrap().start(),
                                    );

                                    for item in &with_stmt.items {
                                        joiner.entry_with_line_separator(
                                            item,
                                            &item.format(),
                                            soft_line_break_or_space(),
                                        );
                                    }
                                    joiner.finish()
                                }))
                                .fmt(f)
                            }

                            WithItemsLayout::Python38OrOlder => f
                                .join_with(format_args![token(","), space()])
                                .entries(with_stmt.items.iter().map(|item| {
                                    item.format().with_options(WithItemLayout::Python38OrOlder {
                                        single: with_stmt.items.len() == 1,
                                    })
                                }))
                                .finish(),

                            WithItemsLayout::Parenthesized => parenthesized(
                                "(",
                                &format_with(|f: &mut PyFormatter| {
                                    f.join_comma_separated(with_stmt.body.first().unwrap().start())
                                        .nodes(&with_stmt.items)
                                        .finish()
                                }),
                                ")",
                            )
                            .with_dangling_comments(parenthesized_comments)
                            .fmt(f),
                        }
                    })
                ),
                clause_body(&with_stmt.body, SuiteKind::other(true), colon_comments)
            ]
        )
    }
}

#[derive(Clone, Copy, Debug)]
enum WithItemsLayout<'a> {
    /// The with statement's only item has a parenthesized context manager.
    ///
    /// ```python
    /// with (
    ///     a + b
    /// ):
    ///     ...
    ///
    /// with (
    ///     a + b
    /// ) as b:
    ///     ...
    /// ```
    ///
    /// In this case, prefer keeping the parentheses around the context expression instead of parenthesizing the entire
    /// with item.
    ///
    /// Ensure that this layout is compatible with [`Self::SingleWithoutTarget`] because removing the parentheses
    /// results in the formatter taking that layout when formatting the file again
    SingleParenthesizedContextManager(&'a WithItem),

    /// The with statement's only item has no target.
    ///
    /// ```python
    /// with a + b:
    ///     ...
    /// ```
    ///
    /// In this case, use [`maybe_parenthesize_expression`] to format the context expression
    /// to get the exact same formatting as when formatting an expression in any other clause header.
    ///
    /// Only used for Python 3.9+
    ///
    /// Be careful that [`Self::SingleParenthesizedContextManager`] and this layout are compatible because
    /// adding parentheses around a [`WithItem`] will result in the context expression being parenthesized in
    /// the next formatting pass.
    SingleWithoutTarget(&'a WithItem),

    /// It's a single with item with a target. Use the optional parentheses layout (see [`optional_parentheses`])
    /// to mimic the `maybe_parenthesize_expression` behavior.
    ///
    /// ```python
    /// with (
    ///     a + b as b
    /// ):
    ///     ...
    /// ```
    ///
    /// Only used for Python 3.9+
    SingleWithTarget(&'a WithItem),

    /// The target python version doesn't support parenthesized context managers because it is Python 3.8 or older.
    ///
    /// In this case, never add parentheses and join the with items with spaces.
    ///
    /// ```python
    /// with ContextManager1(
    ///     aaaaaaaaaaaaaaa, b
    /// ), ContextManager2(), ContextManager3(), ContextManager4():
    ///     pass
    /// ```
    Python38OrOlder,

    /// Wrap the with items in parentheses if they don't fit on a single line and join them by soft line breaks.
    ///
    /// ```python
    /// with (
    ///     ContextManager1(aaaaaaaaaaaaaaa, b),
    ///     ContextManager1(),
    ///     ContextManager1(),
    ///     ContextManager1(),
    /// ):
    ///     pass
    /// ```
    ///
    /// Only used for Python 3.9+.
    ParenthesizeIfExpands,

    /// Always parenthesize because the context managers open-parentheses have a trailing comment:
    ///
    /// ```python
    /// with (  # comment
    ///       CtxManager() as example
    /// ):
    ///    ...
    /// ```
    ///
    /// Or because it is a single item with a trailing or leading comment.
    ///
    /// ```python
    /// with (
    ///    # leading
    ///    CtxManager()
    ///    # trailing
    /// ): pass
    /// ```
    Parenthesized,
}

impl<'a> WithItemsLayout<'a> {
    fn from_statement(
        with: &'a StmtWith,
        context: &PyFormatContext,
        parenthesized_comments: &[SourceComment],
    ) -> FormatResult<Self> {
        // The with statement already has parentheses around the entire with items. Guaranteed to be Python 3.9 or newer
        // ```
        // with (  # comment
        //     CtxManager() as example
        // ):
        //     pass
        // ```
        if !parenthesized_comments.is_empty() {
            return Ok(Self::Parenthesized);
        }

        // A trailing comma at the end guarantees that the context managers are parenthesized and that it is Python 3.9 or newer syntax.
        // ```python
        // with (  # comment
        //     CtxManager() as example,
        // ):
        //     pass
        // ```
        if has_magic_trailing_comma(with, context) {
            return Ok(Self::ParenthesizeIfExpands);
        }

        if let [single] = with.items.as_slice() {
            // If the with item itself has comments (not the context expression), then keep the parentheses
            // ```python
            // with (
            //     # leading
            //     a
            // ): pass
            // ```
            if context.comments().has_leading(single) || context.comments().has_trailing(single) {
                return Ok(Self::Parenthesized);
            }

            // Preserve the parentheses around the context expression instead of parenthesizing the entire
            // with items.
            if is_expression_parenthesized(
                (&single.context_expr).into(),
                context.comments().ranges(),
                context.source(),
            ) {
                return Ok(Self::SingleParenthesizedContextManager(single));
            }
        }

        let can_parenthesize = context.options().target_version() >= PythonVersion::Py39
            || are_with_items_parenthesized(with, context)?;

        // If the target version doesn't support parenthesized context managers and they aren't
        // parenthesized by the user, bail out.
        if !can_parenthesize {
            return Ok(Self::Python38OrOlder);
        }

        Ok(match with.items.as_slice() {
            [single] => {
                if single.optional_vars.is_none() {
                    Self::SingleWithoutTarget(single)
                } else if can_omit_optional_parentheses(&single.context_expr, context) {
                    Self::SingleWithTarget(single)
                } else {
                    Self::ParenthesizeIfExpands
                }
            }
            // Always parenthesize multiple items
            [..] => Self::ParenthesizeIfExpands,
        })
    }
}

fn has_magic_trailing_comma(with: &StmtWith, context: &PyFormatContext) -> bool {
    let Some(last_item) = with.items.last() else {
        return false;
    };

    commas::has_magic_trailing_comma(TextRange::new(last_item.end(), with.end()), context)
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
