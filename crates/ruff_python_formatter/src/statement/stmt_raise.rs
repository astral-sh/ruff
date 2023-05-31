use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::StmtRaise;

#[derive(Default)]
pub(crate) struct FormatStmtRaise;

impl FormatNodeRule<StmtRaise> for FormatStmtRaise {
    fn fmt_fields(&self, _item: &StmtRaise, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
