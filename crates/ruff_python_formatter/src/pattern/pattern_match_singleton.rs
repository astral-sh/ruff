use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::PatternMatchSingleton;

use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchSingleton;

impl FormatNodeRule<PatternMatchSingleton> for FormatPatternMatchSingleton {
    fn fmt_fields(&self, item: &PatternMatchSingleton, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented_custom_text("None", item)])
    }
}
