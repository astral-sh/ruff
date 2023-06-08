use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtAsyncFor;

#[derive(Default)]
pub struct FormatStmtAsyncFor;

impl FormatNodeRule<StmtAsyncFor> for FormatStmtAsyncFor {
    fn fmt_fields(&self, item: &StmtAsyncFor, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
