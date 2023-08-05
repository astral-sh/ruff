use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::StmtLineMagic;

#[derive(Default)]
pub struct FormatStmtLineMagic;

impl FormatNodeRule<StmtLineMagic> for FormatStmtLineMagic {
    fn fmt_fields(&self, item: &StmtLineMagic, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item)])
    }
}
