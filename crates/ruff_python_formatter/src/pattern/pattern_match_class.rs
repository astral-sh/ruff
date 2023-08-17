use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::PatternMatchClass;

use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchClass;

impl FormatNodeRule<PatternMatchClass> for FormatPatternMatchClass {
    fn fmt_fields(&self, item: &PatternMatchClass, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "NOT_YET_IMPLEMENTED_PatternMatchClass(0, 0)",
                item
            )]
        )
    }
}
