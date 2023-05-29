use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::Arg;

#[derive(Default)]
pub struct FormatArg;

impl FormatNodeRule<Arg> for FormatArg {
    fn fmt_fields(&self, _item: &Arg, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
