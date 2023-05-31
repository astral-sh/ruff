use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtAssign;

#[derive(Default)]
pub(crate) struct FormatStmtAssign;

impl FormatNodeRule<StmtAssign> for FormatStmtAssign {
    fn fmt_fields(&self, _item: &StmtAssign, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
