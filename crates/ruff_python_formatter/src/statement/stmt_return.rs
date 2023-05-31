use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtReturn;

#[derive(Default)]
pub(crate) struct FormatStmtReturn;

impl FormatNodeRule<StmtReturn> for FormatStmtReturn {
    fn fmt_fields(&self, _item: &StmtReturn, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
