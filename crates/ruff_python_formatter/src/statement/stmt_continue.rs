use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::text;
use ruff_formatter::{Format, FormatResult};
use ruff_python_ast::StmtContinue;

#[derive(Default)]
pub struct FormatStmtContinue;

impl FormatNodeRule<StmtContinue> for FormatStmtContinue {
    fn fmt_fields(&self, _item: &StmtContinue, f: &mut PyFormatter) -> FormatResult<()> {
        text("continue").fmt(f)
    }
}
