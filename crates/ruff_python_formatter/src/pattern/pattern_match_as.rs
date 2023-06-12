use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::PatternMatchAs;

#[derive(Default)]
pub struct FormatPatternMatchAs;

impl FormatNodeRule<PatternMatchAs> for FormatPatternMatchAs {
    fn fmt_fields(&self, item: &PatternMatchAs, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
