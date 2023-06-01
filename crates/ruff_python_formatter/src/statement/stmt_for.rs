use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtFor;

#[derive(Default)]
pub struct FormatStmtFor;

impl FormatNodeRule<StmtFor> for FormatStmtFor {
    fn fmt_fields(&self, _item: &StmtFor, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
