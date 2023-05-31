use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ExcepthandlerExceptHandler;

#[derive(Default)]
pub(crate) struct FormatExcepthandlerExceptHandler;

impl FormatNodeRule<ExcepthandlerExceptHandler> for FormatExcepthandlerExceptHandler {
    fn fmt_fields(
        &self,
        _item: &ExcepthandlerExceptHandler,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        Ok(())
    }
}
