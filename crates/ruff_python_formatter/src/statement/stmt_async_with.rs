use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtAsyncWith;

#[derive(Default)]
pub(crate) struct FormatStmtAsyncWith;

impl FormatNodeRule<StmtAsyncWith> for FormatStmtAsyncWith {
    fn fmt_fields(&self, _item: &StmtAsyncWith, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
