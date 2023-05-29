use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ModFunctionType;

#[derive(Default)]
pub struct FormatModFunctionType;

impl FormatNodeRule<ModFunctionType> for FormatModFunctionType {
    fn fmt_fields(&self, _item: &ModFunctionType, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
