use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::PatternMatchOr;

#[derive(Default)]
pub struct FormatPatternMatchOr;

impl FormatNodeRule<PatternMatchOr> for FormatPatternMatchOr {
    fn fmt_fields(&self, _item: &PatternMatchOr, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
