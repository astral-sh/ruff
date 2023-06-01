use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtGlobal;

#[derive(Default)]
pub struct FormatStmtGlobal;

impl FormatNodeRule<StmtGlobal> for FormatStmtGlobal {
    fn fmt_fields(&self, item: &StmtGlobal, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
