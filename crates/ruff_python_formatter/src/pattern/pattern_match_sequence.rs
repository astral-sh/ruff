use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::PatternMatchSequence;

#[derive(Default)]
pub struct FormatPatternMatchSequence;

impl FormatNodeRule<PatternMatchSequence> for FormatPatternMatchSequence {
    fn fmt_fields(&self, _item: &PatternMatchSequence, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
