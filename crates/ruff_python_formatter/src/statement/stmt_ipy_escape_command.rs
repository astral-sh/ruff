use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::StmtIpyEscapeCommand;

#[derive(Default)]
pub struct FormatStmtIpyEscapeCommand;

impl FormatNodeRule<StmtIpyEscapeCommand> for FormatStmtIpyEscapeCommand {
    fn fmt_fields(&self, item: &StmtIpyEscapeCommand, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item)])
    }
}
