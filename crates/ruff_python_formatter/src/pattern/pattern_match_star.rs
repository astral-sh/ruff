use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::PatternMatchStar;

use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchStar;

impl FormatNodeRule<PatternMatchStar> for FormatPatternMatchStar {
    fn fmt_fields(&self, item: &PatternMatchStar, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "*NOT_YET_IMPLEMENTED_PatternMatchStar",
                item
            )]
        )
    }
}
