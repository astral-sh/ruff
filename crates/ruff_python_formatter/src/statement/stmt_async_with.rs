use rustpython_parser::ast::{Stmt, StmtAsyncWith, WithItem};

use ruff_formatter::FormatResult;

use super::stmt_with::FormatWithLike;
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatStmtAsyncWith;

impl<'ast> FormatWithLike<'ast> for StmtAsyncWith {
    const ASYNC: bool = true;

    fn destruct(&self) -> (&Vec<WithItem>, &Vec<Stmt>) {
        (&self.items, &self.body)
    }
}

impl FormatNodeRule<StmtAsyncWith> for FormatStmtAsyncWith {
    fn fmt_fields(&self, item: &StmtAsyncWith, f: &mut PyFormatter) -> FormatResult<()> {
        item.fmt_with(f)
    }

    fn fmt_dangling_comments(
        &self,
        _node: &StmtAsyncWith,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
