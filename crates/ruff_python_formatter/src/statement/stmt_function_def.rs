use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtFunctionDef;

#[derive(Default)]
pub struct FormatStmtFunctionDef;

impl FormatNodeRule<StmtFunctionDef> for FormatStmtFunctionDef {
    fn fmt_fields(&self, _item: &StmtFunctionDef, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
