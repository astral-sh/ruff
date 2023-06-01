use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtWhile;

#[derive(Default)]
pub struct FormatStmtWhile;

impl FormatNodeRule<StmtWhile> for FormatStmtWhile {
    fn fmt_fields(&self, item: &StmtWhile, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
