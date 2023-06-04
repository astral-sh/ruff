use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtPass;

#[derive(Default)]
pub struct FormatStmtPass;

impl FormatNodeRule<StmtPass> for FormatStmtPass {
    fn fmt_fields(&self, item: &StmtPass, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
