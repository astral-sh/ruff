use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::PatternMatchSingleton;

#[derive(Default)]
pub struct FormatPatternMatchSingleton;

impl FormatNodeRule<PatternMatchSingleton> for FormatPatternMatchSingleton {
    fn fmt_fields(&self, item: &PatternMatchSingleton, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
