use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::text;
use ruff_formatter::{Format, FormatResult};
use rustpython_parser::ast::StmtContinue;

#[derive(Default)]
pub struct FormatStmtContinue;

impl FormatNodeRule<StmtContinue> for FormatStmtContinue {
    fn fmt_fields(&self, _item: &StmtContinue, f: &mut PyFormatter) -> FormatResult<()> {
        text("continue").fmt(f)
    }
}
