use crate::prelude::*;
use crate::verbatim_text;
use ruff_formatter::write;
use ruff_python_ast::FTStringFormatSpec;

#[derive(Default)]
pub struct FormatFTStringFormatSpec;

impl FormatNodeRule<FTStringFormatSpec> for FormatFTStringFormatSpec {
    fn fmt_fields(&self, item: &FTStringFormatSpec, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item)])
    }
}
