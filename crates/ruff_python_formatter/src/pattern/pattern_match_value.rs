use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::PatternMatchValue;

use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchValue;

impl FormatNodeRule<PatternMatchValue> for FormatPatternMatchValue {
    fn fmt_fields(&self, item: &PatternMatchValue, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "\"NOT_YET_IMPLEMENTED_PatternMatchValue\"",
                item
            )]
        )
    }
}
