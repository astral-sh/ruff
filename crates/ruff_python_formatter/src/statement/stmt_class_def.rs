use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtClassDef;

#[derive(Default)]
pub struct FormatStmtClassDef;

impl FormatNodeRule<StmtClassDef> for FormatStmtClassDef {
    fn fmt_fields(&self, _item: &StmtClassDef, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
