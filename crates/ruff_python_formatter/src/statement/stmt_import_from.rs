use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtImportFrom;

#[derive(Default)]
pub struct FormatStmtImportFrom;

impl FormatNodeRule<StmtImportFrom> for FormatStmtImportFrom {
    fn fmt_fields(&self, _item: &StmtImportFrom, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
