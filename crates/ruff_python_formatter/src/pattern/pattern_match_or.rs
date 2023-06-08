use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::PatternMatchOr;

#[derive(Default)]
pub struct FormatPatternMatchOr;

impl FormatNodeRule<PatternMatchOr> for FormatPatternMatchOr {
    fn fmt_fields(&self, item: &PatternMatchOr, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
