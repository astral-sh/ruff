use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtAssert;

#[derive(Default)]
pub struct FormatStmtAssert;

impl FormatNodeRule<StmtAssert> for FormatStmtAssert {
    fn fmt_fields(&self, _item: &StmtAssert, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
