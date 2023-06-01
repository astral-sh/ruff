use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::Arg;

#[derive(Default)]
pub struct FormatArg;

impl FormatNodeRule<Arg> for FormatArg {
    fn fmt_fields(&self, item: &Arg, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
