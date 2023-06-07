use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::TypeIgnoreTypeIgnore;

#[derive(Default)]
pub struct FormatTypeIgnoreTypeIgnore;

impl FormatNodeRule<TypeIgnoreTypeIgnore> for FormatTypeIgnoreTypeIgnore {
    fn fmt_fields(&self, item: &TypeIgnoreTypeIgnore, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
