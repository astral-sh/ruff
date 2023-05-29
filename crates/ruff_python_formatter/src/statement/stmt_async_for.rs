use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtAsyncFor;

#[derive(Default)]
pub struct FormatStmtAsyncFor;

impl FormatNodeRule<StmtAsyncFor> for FormatStmtAsyncFor {
    fn fmt_fields(&self, _item: &StmtAsyncFor, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
