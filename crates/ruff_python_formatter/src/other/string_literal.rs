use ruff_python_ast::StringLiteral;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatStringLiteral;

impl FormatNodeRule<StringLiteral> for FormatStringLiteral {
    fn fmt_fields(&self, item: &StringLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        unreachable!("Handled inside of `FormatExprStringLiteral`");
    }
}
