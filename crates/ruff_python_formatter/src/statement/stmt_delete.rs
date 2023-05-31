use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtDelete;

#[derive(Default)]
pub(crate) struct FormatStmtDelete;

impl FormatNodeRule<StmtDelete> for FormatStmtDelete {
    fn fmt_fields(&self, _item: &StmtDelete, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
