use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExcepthandlerExceptHandler;

#[derive(Default)]
pub struct FormatExcepthandlerExceptHandler;

impl FormatNodeRule<ExcepthandlerExceptHandler> for FormatExcepthandlerExceptHandler {
    fn fmt_fields(
        &self,
        item: &ExcepthandlerExceptHandler,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
