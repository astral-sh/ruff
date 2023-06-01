use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::StmtRaise;

#[derive(Default)]
pub struct FormatStmtRaise;

impl FormatNodeRule<StmtRaise> for FormatStmtRaise {
    fn fmt_fields(&self, item: &StmtRaise, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
