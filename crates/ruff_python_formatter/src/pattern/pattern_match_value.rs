use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::PatternMatchValue;

use crate::{AsFormat, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchValue;

impl FormatNodeRule<PatternMatchValue> for FormatPatternMatchValue {
    fn fmt_fields(&self, item: &PatternMatchValue, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchValue { value, .. } = item;

        let formatted = value.format();

        write!(f, [formatted])
    }
}
