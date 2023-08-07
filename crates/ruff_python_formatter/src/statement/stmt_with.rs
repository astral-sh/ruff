use ruff_formatter::{format_args, write, FormatError};
use ruff_python_ast::{Ranged, StmtWith};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::TextRange;

use crate::comments::trailing_comments;
use crate::expression::parentheses::{
    in_parentheses_only_soft_line_break_or_space, optional_parentheses,
};
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtWith;

impl FormatNodeRule<StmtWith> for FormatStmtWith {
    fn fmt_fields(&self, item: &StmtWith, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling_comments(item);

        write!(
            f,
            [
                item.is_async
                    .then_some(format_args![text("async"), space()]),
                text("with"),
                space()
            ]
        )?;

        if are_with_items_parenthesized(item, f.context())? {
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

        write!(
            f,
            [
                text(":"),
                trailing_comments(dangling_comments),
                block_indent(&item.body.format())
            ]
        )
    }

    fn fmt_dangling_comments(&self, _node: &StmtWith, _f: &mut PyFormatter) -> FormatResult<()> {
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
