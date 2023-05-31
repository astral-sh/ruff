use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtAssert;

#[derive(Default)]
pub(crate) struct FormatStmtAssert;

impl FormatNodeRule<StmtAssert> for FormatStmtAssert {
    fn fmt_fields(&self, _item: &StmtAssert, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
