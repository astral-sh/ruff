use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ModFunctionType;

#[derive(Default)]
pub struct FormatModFunctionType;

impl FormatNodeRule<ModFunctionType> for FormatModFunctionType {
    fn fmt_fields(&self, item: &ModFunctionType, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
