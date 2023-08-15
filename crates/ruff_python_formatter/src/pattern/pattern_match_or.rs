use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::PatternMatchOr;

use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchOr;

impl FormatNodeRule<PatternMatchOr> for FormatPatternMatchOr {
    fn fmt_fields(&self, item: &PatternMatchOr, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "NOT_YET_IMPLEMENTED_PatternMatchOf | (y)",
                item
            )]
        )
    }
}
