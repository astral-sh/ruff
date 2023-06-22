use rustpython_parser::ast::PatternMatchValue;

use ruff_formatter::{write, Buffer, FormatResult};

use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchValue;

impl FormatNodeRule<PatternMatchValue> for FormatPatternMatchValue {
    fn fmt_fields(&self, item: &PatternMatchValue, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
