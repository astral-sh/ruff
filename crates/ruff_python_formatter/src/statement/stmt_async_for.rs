use crate::prelude::*;
use crate::statement::stmt_for::AnyStatementFor;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use ruff_python_ast::StmtAsyncFor;

#[derive(Default)]
pub struct FormatStmtAsyncFor;

impl FormatNodeRule<StmtAsyncFor> for FormatStmtAsyncFor {
    fn fmt_fields(&self, item: &StmtAsyncFor, f: &mut PyFormatter) -> FormatResult<()> {
        AnyStatementFor::from(item).fmt(f)
    }

    fn fmt_dangling_comments(
        &self,
        _node: &StmtAsyncFor,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
