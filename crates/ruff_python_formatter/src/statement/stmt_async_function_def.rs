use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtAsyncFunctionDef;

#[derive(Default)]
pub(crate) struct FormatStmtAsyncFunctionDef;

impl FormatNodeRule<StmtAsyncFunctionDef> for FormatStmtAsyncFunctionDef {
    fn fmt_fields(&self, _item: &StmtAsyncFunctionDef, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
