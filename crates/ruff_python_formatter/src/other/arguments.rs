use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::Arguments;

#[derive(Default)]
pub(crate) struct FormatArguments;

impl FormatNodeRule<Arguments> for FormatArguments {
    fn fmt_fields(&self, _item: &Arguments, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
