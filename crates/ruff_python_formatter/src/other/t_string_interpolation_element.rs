use crate::prelude::*;
use crate::verbatim_text;
use ruff_formatter::write;
use ruff_python_ast::TStringInterpolationElement;

#[derive(Default)]
pub struct FormatTStringInterpolationElement;

impl FormatNodeRule<TStringInterpolationElement> for FormatTStringInterpolationElement {
    fn fmt_fields(
        &self,
        item: &TStringInterpolationElement,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        write!(f, [verbatim_text(item)])
    }
}
