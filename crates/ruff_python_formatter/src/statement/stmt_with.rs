use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{Ranged, StmtAsyncWith, StmtWith, Suite, WithItem};

use crate::builders::optional_parentheses;
use crate::comments::trailing_comments;
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

        let joined_items =
            format_with(|f| f.join_comma_separated().nodes(self.items().iter()).finish());

        if self.is_async() {
            write!(f, [text("async"), space()])?;
        }

        write!(
            f,
            [
                text("with"),
                space(),
                group(&optional_parentheses(&joined_items)),
                text(":"),
                trailing_comments(dangling_comments),
                block_indent(&self.body().format())
            ]
        )
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
