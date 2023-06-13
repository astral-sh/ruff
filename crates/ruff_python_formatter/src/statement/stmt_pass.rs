use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::text;
use ruff_formatter::{Format, FormatResult};
use rustpython_parser::ast::StmtPass;

#[derive(Default)]
pub struct FormatStmtPass;

impl FormatNodeRule<StmtPass> for FormatStmtPass {
    fn fmt_fields(&self, _item: &StmtPass, f: &mut PyFormatter) -> FormatResult<()> {
        text("pass").fmt(f)
    }
}
