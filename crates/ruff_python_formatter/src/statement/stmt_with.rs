use ruff_formatter::{format_args, write, FormatError};
use ruff_python_ast::node::AstNode;
use ruff_python_ast::{Ranged, StmtWith};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::TextRange;

use crate::comments::{trailing_comments, SourceComment, SuppressionKind};
use crate::expression::parentheses::{
    in_parentheses_only_soft_line_break_or_space, optional_parentheses, parenthesized,
};
use crate::prelude::*;
use crate::verbatim::SuppressedClauseHeader;
use crate::FormatNodeRule;

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
        let dangling_comments = comments.dangling_comments(item.as_any_node_ref());
        let partition_point = dangling_comments.partition_point(|comment| {
            item.items
                .first()
                .is_some_and(|with_item| with_item.start() > comment.start())
        });
        let (parenthesized_comments, colon_comments) = dangling_comments.split_at(partition_point);

        if SuppressionKind::has_skip_comment(colon_comments, f.context().source()) {
            SuppressedClauseHeader::With(item).fmt(f)?;
        } else {
            write!(
                f,
                [
                    item.is_async
                        .then_some(format_args![text("async"), space()]),
                    text("with"),
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
            } else if are_with_items_parenthesized(item, f.context())? {
                optional_parentheses(&format_with(|f| {
                    let mut joiner = f.join_comma_separated(item.body.first().unwrap().start());

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
            } else {
                f.join_with(format_args![text(","), space()])
                    .entries(item.items.iter().formatted())
                    .finish()?;
            }

            text(":").fmt(f)?;
        }

        write!(
            f,
            [
                trailing_comments(colon_comments),
                block_indent(&item.body.format())
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

fn are_with_items_parenthesized(with: &StmtWith, context: &PyFormatContext) -> FormatResult<bool> {
    let first_with_item = with
        .items
        .first()
        .ok_or(FormatError::syntax_error("Expected at least one with item"))?;
    let before_first_with_item = TextRange::new(with.start(), first_with_item.start());

    let mut tokenizer = SimpleTokenizer::new(context.source(), before_first_with_item)
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
