use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtAsyncWith;

#[derive(Default)]
pub struct FormatStmtAsyncWith;

impl FormatNodeRule<StmtAsyncWith> for FormatStmtAsyncWith {
    fn fmt_fields(&self, item: &StmtAsyncWith, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
