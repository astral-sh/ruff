use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtImport;

#[derive(Default)]
pub struct FormatStmtImport;

impl FormatNodeRule<StmtImport> for FormatStmtImport {
    fn fmt_fields(&self, _item: &StmtImport, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
