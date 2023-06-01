use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::PatternMatchMapping;

#[derive(Default)]
pub struct FormatPatternMatchMapping;

impl FormatNodeRule<PatternMatchMapping> for FormatPatternMatchMapping {
    fn fmt_fields(&self, _item: &PatternMatchMapping, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
