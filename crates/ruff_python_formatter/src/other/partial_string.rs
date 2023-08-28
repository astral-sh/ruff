use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use ruff_python_ast::FStringLiteralElement;

#[derive(Default)]
pub struct FormatFStringLiteralElement;

impl FormatNodeRule<FStringLiteralElement> for FormatFStringLiteralElement {
    fn fmt_fields(&self, _item: &FStringLiteralElement, _f: &mut PyFormatter) -> FormatResult<()> {
        unreachable!("Handled inside of `FormatExprFString");
    }
}
