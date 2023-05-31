use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtAsyncWith;

#[derive(Default)]
pub struct FormatStmtAsyncWith;

impl FormatNodeRule<StmtAsyncWith> for FormatStmtAsyncWith {
    fn fmt_fields(&self, item: &StmtAsyncWith, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
