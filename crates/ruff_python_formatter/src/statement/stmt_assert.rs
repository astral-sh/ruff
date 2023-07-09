use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtAssert;

#[derive(Default)]
pub struct FormatStmtAssert;

impl FormatNodeRule<StmtAssert> for FormatStmtAssert {
    fn fmt_fields(&self, item: &StmtAssert, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
