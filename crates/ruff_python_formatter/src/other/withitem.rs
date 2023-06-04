use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::Withitem;

#[derive(Default)]
pub struct FormatWithitem;

impl FormatNodeRule<Withitem> for FormatWithitem {
    fn fmt_fields(&self, item: &Withitem, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
