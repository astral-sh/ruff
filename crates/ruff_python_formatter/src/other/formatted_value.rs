use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use ruff_python_ast::FormattedValue;

#[derive(Default)]
pub struct FormatFormattedValue;

impl FormatNodeRule<FormattedValue> for FormatFormattedValue {
    fn fmt_fields(&self, _item: &FormattedValue, _f: &mut PyFormatter) -> FormatResult<()> {
        unreachable!("Handled inside of `FormatExprFString");
    }
}
