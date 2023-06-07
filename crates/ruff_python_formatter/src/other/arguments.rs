use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::Arguments;

#[derive(Default)]
pub struct FormatArguments;

impl FormatNodeRule<Arguments> for FormatArguments {
    fn fmt_fields(&self, item: &Arguments, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
