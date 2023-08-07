use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::ExprIpyEscapeCommand;

#[derive(Default)]
pub struct FormatExprIpyEscapeCommand;

impl FormatNodeRule<ExprIpyEscapeCommand> for FormatExprIpyEscapeCommand {
    fn fmt_fields(&self, item: &ExprIpyEscapeCommand, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item)])
    }
}
