use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::PatternMatchSequence;

use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchSequence;

impl FormatNodeRule<PatternMatchSequence> for FormatPatternMatchSequence {
    fn fmt_fields(&self, item: &PatternMatchSequence, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "[NOT_YET_IMPLEMENTED_PatternMatchSequence, 2]",
                item
            )]
        )
    }
}
