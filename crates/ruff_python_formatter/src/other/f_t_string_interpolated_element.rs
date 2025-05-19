use crate::prelude::*;
use crate::verbatim_text;
use ruff_formatter::write;
use ruff_python_ast::FTStringInterpolatedElement;

#[derive(Default)]
pub struct FormatFTStringInterpolatedElement;

impl FormatNodeRule<FTStringInterpolatedElement> for FormatFTStringInterpolatedElement {
    fn fmt_fields(
        &self,
        item: &FTStringInterpolatedElement,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        write!(f, [verbatim_text(item)])
    }
}
