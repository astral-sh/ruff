use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use ruff_python_ast::FStringExpressionElement;

#[derive(Default)]
pub struct FormatFStringExpressionElement;

impl FormatNodeRule<FStringExpressionElement> for FormatFStringExpressionElement {
    fn fmt_fields(
        &self,
        _item: &FStringExpressionElement,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        unreachable!("Handled inside of `FormatExprFString");
    }
}
