use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::Comprehension;

#[derive(Default)]
pub struct FormatComprehension;

impl FormatNodeRule<Comprehension> for FormatComprehension {
    fn fmt_fields(&self, _item: &Comprehension, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
