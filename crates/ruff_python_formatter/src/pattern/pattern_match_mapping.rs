use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::PatternMatchMapping;

use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchMapping;

impl FormatNodeRule<PatternMatchMapping> for FormatPatternMatchMapping {
    fn fmt_fields(&self, item: &PatternMatchMapping, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "{\"NOT_YET_IMPLEMENTED_PatternMatchMapping\": _, 2: _}",
                item
            )]
        )
    }
}
