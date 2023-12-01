use ruff_python_ast::FString;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatFString;

impl FormatNodeRule<FString> for FormatFString {
    fn fmt_fields(&self, _item: &FString, _f: &mut PyFormatter) -> FormatResult<()> {
        unreachable!("Handled inside of `FormatExprFString`");
    }
}
