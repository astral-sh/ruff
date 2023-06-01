use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ModFunctionType;

#[derive(Default)]
pub struct FormatModFunctionType;

impl FormatNodeRule<ModFunctionType> for FormatModFunctionType {
    fn fmt_fields(&self, item: &ModFunctionType, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
