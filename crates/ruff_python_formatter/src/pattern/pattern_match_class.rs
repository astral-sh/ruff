use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::PatternMatchClass;

#[derive(Default)]
pub struct FormatPatternMatchClass;

impl FormatNodeRule<PatternMatchClass> for FormatPatternMatchClass {
    fn fmt_fields(&self, item: &PatternMatchClass, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
