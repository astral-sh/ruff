use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtIf;

#[derive(Default)]
pub struct FormatStmtIf;

impl FormatNodeRule<StmtIf> for FormatStmtIf {
    fn fmt_fields(&self, _item: &StmtIf, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
