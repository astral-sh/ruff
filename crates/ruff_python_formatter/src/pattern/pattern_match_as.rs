use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::PatternMatchAs;

#[derive(Default)]
pub(crate) struct FormatPatternMatchAs;

impl FormatNodeRule<PatternMatchAs> for FormatPatternMatchAs {
    fn fmt_fields(&self, _item: &PatternMatchAs, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
