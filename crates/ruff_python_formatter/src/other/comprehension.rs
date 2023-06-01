use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::Comprehension;

#[derive(Default)]
pub struct FormatComprehension;

impl FormatNodeRule<Comprehension> for FormatComprehension {
    fn fmt_fields(&self, item: &Comprehension, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
