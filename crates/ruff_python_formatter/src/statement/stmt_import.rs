use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtImport;

#[derive(Default)]
pub struct FormatStmtImport;

impl FormatNodeRule<StmtImport> for FormatStmtImport {
    fn fmt_fields(&self, item: &StmtImport, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
