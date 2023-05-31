use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::PatternMatchSingleton;

#[derive(Default)]
pub(crate) struct FormatPatternMatchSingleton;

impl FormatNodeRule<PatternMatchSingleton> for FormatPatternMatchSingleton {
    fn fmt_fields(&self, _item: &PatternMatchSingleton, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
