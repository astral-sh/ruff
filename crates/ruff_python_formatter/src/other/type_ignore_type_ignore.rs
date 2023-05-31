use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::TypeIgnoreTypeIgnore;

#[derive(Default)]
pub(crate) struct FormatTypeIgnoreTypeIgnore;

impl FormatNodeRule<TypeIgnoreTypeIgnore> for FormatTypeIgnoreTypeIgnore {
    fn fmt_fields(&self, _item: &TypeIgnoreTypeIgnore, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
