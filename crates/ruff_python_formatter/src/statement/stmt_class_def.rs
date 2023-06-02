use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtClassDef;

#[derive(Default)]
pub struct FormatStmtClassDef;

impl FormatNodeRule<StmtClassDef> for FormatStmtClassDef {
    fn fmt_fields(&self, item: &StmtClassDef, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
