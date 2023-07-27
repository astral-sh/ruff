use ruff_python_ast::{Ranged, StmtAsyncWith, StmtWith, Suite, WithItem};
use ruff_text_size::TextRange;

use ruff_formatter::{format_args, write, FormatError};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};

use crate::comments::trailing_comments;
use crate::expression::parentheses::{
    in_parentheses_only_soft_line_break_or_space, optional_parentheses,
};
use crate::prelude::*;
use crate::FormatNodeRule;

pub(super) enum AnyStatementWith<'a> {
    With(&'a StmtWith),
    AsyncWith(&'a StmtAsyncWith),
}

impl<'a> AnyStatementWith<'a> {
    const fn is_async(&self) -> bool {
        matches!(self, AnyStatementWith::AsyncWith(_))
    }

    fn items(&self) -> &[WithItem] {
        match self {
            AnyStatementWith::With(with) => with.items.as_slice(),
            AnyStatementWith::AsyncWith(with) => with.items.as_slice(),
        }
    }

    fn body(&self) -> &Suite {
        match self {
            AnyStatementWith::With(with) => &with.body,
            AnyStatementWith::AsyncWith(with) => &with.body,
        }
    }
}

impl Ranged for AnyStatementWith<'_> {
    fn range(&self) -> TextRange {
        match self {
            AnyStatementWith::With(with) => with.range(),
            AnyStatementWith::AsyncWith(with) => with.range(),
        }
    }
}

impl<'a> From<&'a StmtWith> for AnyStatementWith<'a> {
    fn from(value: &'a StmtWith) -> Self {
        AnyStatementWith::With(value)
    }
}

impl<'a> From<&'a StmtAsyncWith> for AnyStatementWith<'a> {
    fn from(value: &'a StmtAsyncWith) -> Self {
        AnyStatementWith::AsyncWith(value)
    }
}

impl<'a> From<&AnyStatementWith<'a>> for AnyNodeRef<'a> {
    fn from(value: &AnyStatementWith<'a>) -> Self {
        match value {
            AnyStatementWith::With(with) => AnyNodeRef::StmtWith(with),
            AnyStatementWith::AsyncWith(with) => AnyNodeRef::StmtAsyncWith(with),
        }
    }
}

impl Format<PyFormatContext<'_>> for AnyStatementWith<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling_comments(self);

        write!(
            f,
            [
                self.is_async()
                    .then_some(format_args![text("async"), space()]),
                text("with"),
                space()
            ]
        )?;

        if are_with_items_parenthesized(self, f.context())? {
            optional_parentheses(&format_with(|f| {
                let mut joiner = f.join_comma_separated(self.body().first().unwrap().start());

                for item in self.items() {
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
                .entries(self.items().iter().formatted())
                .finish()?;
        }

        write!(
            f,
            [
                text(":"),
                trailing_comments(dangling_comments),
                block_indent(&self.body().format())
            ]
        )
    }
}

fn are_with_items_parenthesized(
    with: &AnyStatementWith,
    context: &PyFormatContext,
) -> FormatResult<bool> {
    let first_with_item = with
        .items()
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

#[derive(Default)]
pub struct FormatStmtWith;

impl FormatNodeRule<StmtWith> for FormatStmtWith {
    fn fmt_fields(&self, item: &StmtWith, f: &mut PyFormatter) -> FormatResult<()> {
        AnyStatementWith::from(item).fmt(f)
    }

    fn fmt_dangling_comments(&self, _node: &StmtWith, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
