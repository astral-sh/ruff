use ruff_python_ast::BytesLiteral;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatBytesLiteral;

impl FormatNodeRule<BytesLiteral> for FormatBytesLiteral {
    fn fmt_fields(&self, _item: &BytesLiteral, _f: &mut PyFormatter) -> FormatResult<()> {
        unreachable!("Handled inside of `FormatExprBytesLiteral`");
    }
}
