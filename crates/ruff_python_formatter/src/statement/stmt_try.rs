use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtTry;

#[derive(Default)]
pub struct FormatStmtTry;

impl FormatNodeRule<StmtTry> for FormatStmtTry {
    fn fmt_fields(&self, _item: &StmtTry, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
