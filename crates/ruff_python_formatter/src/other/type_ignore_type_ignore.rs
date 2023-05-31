use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::TypeIgnoreTypeIgnore;

#[derive(Default)]
pub struct FormatTypeIgnoreTypeIgnore;

impl FormatNodeRule<TypeIgnoreTypeIgnore> for FormatTypeIgnoreTypeIgnore {
    fn fmt_fields(&self, item: &TypeIgnoreTypeIgnore, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
