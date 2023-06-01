use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtNonlocal;

#[derive(Default)]
pub struct FormatStmtNonlocal;

impl FormatNodeRule<StmtNonlocal> for FormatStmtNonlocal {
    fn fmt_fields(&self, _item: &StmtNonlocal, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
