use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::PatternMatchAs;

use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchAs;

impl FormatNodeRule<PatternMatchAs> for FormatPatternMatchAs {
    fn fmt_fields(&self, item: &PatternMatchAs, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "x as NOT_YET_IMPLEMENTED_PatternMatchAs",
                item
            )]
        )
    }
}
