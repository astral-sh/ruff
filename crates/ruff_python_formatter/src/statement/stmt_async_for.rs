use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtAsyncFor;

#[derive(Default)]
pub struct FormatStmtAsyncFor;

impl FormatNodeRule<StmtAsyncFor> for FormatStmtAsyncFor {
    fn fmt_fields(&self, item: &StmtAsyncFor, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
