use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtWith;

#[derive(Default)]
pub(crate) struct FormatStmtWith;

impl FormatNodeRule<StmtWith> for FormatStmtWith {
    fn fmt_fields(&self, _item: &StmtWith, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
