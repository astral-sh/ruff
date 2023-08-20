use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use ruff_python_ast::PartialString;

#[derive(Default)]
pub struct FormatPartialString;

impl FormatNodeRule<PartialString> for FormatPartialString {
    fn fmt_fields(&self, _item: &PartialString, _f: &mut PyFormatter) -> FormatResult<()> {
        unreachable!("Handled inside of `FormatExprFString");
    }
}
