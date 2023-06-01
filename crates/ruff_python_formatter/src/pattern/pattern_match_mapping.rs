use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::PatternMatchMapping;

#[derive(Default)]
pub struct FormatPatternMatchMapping;

impl FormatNodeRule<PatternMatchMapping> for FormatPatternMatchMapping {
    fn fmt_fields(&self, item: &PatternMatchMapping, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
