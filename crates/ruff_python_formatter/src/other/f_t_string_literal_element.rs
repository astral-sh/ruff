use crate::prelude::*;
use crate::verbatim_text;
use ruff_formatter::write;
use ruff_python_ast::FTStringLiteralElement;

#[derive(Default)]
pub struct FormatFTStringLiteralElement;

impl FormatNodeRule<FTStringLiteralElement> for FormatFTStringLiteralElement {
    fn fmt_fields(&self, item: &FTStringLiteralElement, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item)])
    }
}
