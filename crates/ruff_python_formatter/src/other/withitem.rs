use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::Withitem;

#[derive(Default)]
pub struct FormatWithitem;

impl FormatNodeRule<Withitem> for FormatWithitem {
    fn fmt_fields(&self, item: &Withitem, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
