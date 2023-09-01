use ruff_formatter::{format_args, write, FormatError};
use ruff_python_ast::node::AstNode;
use ruff_python_ast::StmtWith;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::builders::parenthesize_if_expands;
use crate::comments::SourceComment;
use crate::expression::parentheses::{
    in_parentheses_only_soft_line_break_or_space, optional_parentheses, parenthesized,
};
use crate::other::commas;
use crate::prelude::*;
use crate::statement::clause::{clause_body, clause_header, ClauseHeader};
use crate::PyFormatOptions;

#[derive(Default)]
pub struct FormatStmtWith;

impl FormatNodeRule<StmtWith> for FormatStmtWith {
    fn fmt_fields(&self, item: &StmtWith, f: &mut PyFormatter) -> FormatResult<()> {
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
        let dangling_comments = comments.dangling(item.as_any_node_ref());
        let partition_point = dangling_comments.partition_point(|comment| {
            item.items
                .first()
                .is_some_and(|with_item| with_item.start() > comment.start())
        });
        let (parenthesized_comments, colon_comments) = dangling_comments.split_at(partition_point);

        write!(
            f,
            [
                clause_header(
                    ClauseHeader::With(item),
                    colon_comments,
                    &format_with(|f| {
                        write!(
                            f,
                            [
                                item.is_async
                                    .then_some(format_args![token("async"), space()]),
                                token("with"),
                                space()
                            ]
                        )?;

                        if !parenthesized_comments.is_empty() {
                            let joined = format_with(|f: &mut PyFormatter| {
                                f.join_comma_separated(item.body.first().unwrap().start())
                                    .nodes(&item.items)
                                    .finish()
                            });

                            parenthesized("(", &joined, ")")
                                .with_dangling_comments(parenthesized_comments)
                                .fmt(f)?;
                        } else if should_parenthesize(item, f.options(), f.context())? {
                            parenthesize_if_expands(&format_with(|f| {
                                let mut joiner =
                                    f.join_comma_separated(item.body.first().unwrap().start());

                                for item in &item.items {
                                    joiner.entry_with_line_separator(
                                        item,
                                        &item.format(),
                                        in_parentheses_only_soft_line_break_or_space(),
                                    );
                                }
                                joiner.finish()
                            }))
                            .fmt(f)?;
                        } else if let [item] = item.items.as_slice() {
                            // This is similar to `maybe_parenthesize_expression`, but we're not dealing with an
                            // expression here, it's a `WithItem`.
                            if comments.has_leading(item) || comments.has_trailing_own_line(item) {
                                optional_parentheses(&item.format()).fmt(f)?;
                            } else {
                                item.format().fmt(f)?;
                            }
                        } else {
                            f.join_with(format_args![token(","), space()])
                                .entries(item.items.iter().formatted())
                                .finish()?;
                        }

                        Ok(())
                    })
                ),
                clause_body(&item.body, colon_comments)
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

/// Returns `true` if the `with` items should be parenthesized, if at least one item expands.
///
/// Black parenthesizes `with` items if there's more than one item and they're already
/// parenthesized, _or_ there's a single item with a trailing comma.
fn should_parenthesize(
    with: &StmtWith,
    options: &PyFormatOptions,
    context: &PyFormatContext,
) -> FormatResult<bool> {
    if has_magic_trailing_comma(with, options, context) {
        return Ok(true);
    }

    if are_with_items_parenthesized(with, context)? {
        return Ok(true);
    }

    Ok(false)
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
