use ruff_python_ast::StringLiteral;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatStringLiteral;

impl FormatNodeRule<StringLiteral> for FormatStringLiteral {
    fn fmt_fields(&self, _item: &StringLiteral, _f: &mut PyFormatter) -> FormatResult<()> {
        unreachable!("Handled inside of `FormatExprStringLiteral`");
    }
}
