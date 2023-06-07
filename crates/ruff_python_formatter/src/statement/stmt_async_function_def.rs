use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtAsyncFunctionDef;

#[derive(Default)]
pub struct FormatStmtAsyncFunctionDef;

impl FormatNodeRule<StmtAsyncFunctionDef> for FormatStmtAsyncFunctionDef {
    fn fmt_fields(&self, item: &StmtAsyncFunctionDef, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
