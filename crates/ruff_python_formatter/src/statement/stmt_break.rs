use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::text;
use ruff_formatter::{Format, FormatResult};
use rustpython_parser::ast::StmtBreak;

#[derive(Default)]
pub struct FormatStmtBreak;

impl FormatNodeRule<StmtBreak> for FormatStmtBreak {
    fn fmt_fields(&self, _item: &StmtBreak, f: &mut PyFormatter) -> FormatResult<()> {
        text("break").fmt(f)
    }
}
