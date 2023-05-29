use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::PatternMatchStar;

#[derive(Default)]
pub struct FormatPatternMatchStar;

impl FormatNodeRule<PatternMatchStar> for FormatPatternMatchStar {
    fn fmt_fields(&self, _item: &PatternMatchStar, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
