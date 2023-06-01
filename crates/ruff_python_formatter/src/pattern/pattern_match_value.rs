use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::PatternMatchValue;

#[derive(Default)]
pub struct FormatPatternMatchValue;

impl FormatNodeRule<PatternMatchValue> for FormatPatternMatchValue {
    fn fmt_fields(&self, _item: &PatternMatchValue, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
