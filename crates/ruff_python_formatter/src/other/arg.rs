use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::Arg;

#[derive(Default)]
pub struct FormatArg;

impl FormatNodeRule<Arg> for FormatArg {
    fn fmt_fields(&self, item: &Arg, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
