use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtWhile;

#[derive(Default)]
pub(crate) struct FormatStmtWhile;

impl FormatNodeRule<StmtWhile> for FormatStmtWhile {
    fn fmt_fields(&self, _item: &StmtWhile, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
