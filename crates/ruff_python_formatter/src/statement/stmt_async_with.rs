use crate::prelude::*;
use crate::statement::stmt_with::AnyStatementWith;
use crate::FormatNodeRule;
use rustpython_parser::ast::StmtAsyncWith;

#[derive(Default)]
pub struct FormatStmtAsyncWith;

impl FormatNodeRule<StmtAsyncWith> for FormatStmtAsyncWith {
    fn fmt_fields(&self, item: &StmtAsyncWith, f: &mut PyFormatter) -> FormatResult<()> {
        AnyStatementWith::from(item).fmt(f)
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
