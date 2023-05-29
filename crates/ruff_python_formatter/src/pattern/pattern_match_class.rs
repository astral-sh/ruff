use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::PatternMatchClass;

#[derive(Default)]
pub struct FormatPatternMatchClass;

impl FormatNodeRule<PatternMatchClass> for FormatPatternMatchClass {
    fn fmt_fields(&self, _item: &PatternMatchClass, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
