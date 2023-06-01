use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtGlobal;

#[derive(Default)]
pub struct FormatStmtGlobal;

impl FormatNodeRule<StmtGlobal> for FormatStmtGlobal {
    fn fmt_fields(&self, _item: &StmtGlobal, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
