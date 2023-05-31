use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtContinue;

#[derive(Default)]
pub(crate) struct FormatStmtContinue;

impl FormatNodeRule<StmtContinue> for FormatStmtContinue {
    fn fmt_fields(&self, _item: &StmtContinue, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
