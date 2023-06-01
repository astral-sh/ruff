use crate::{verbatim_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ModInteractive;

#[derive(Default)]
pub struct FormatModInteractive;

impl FormatNodeRule<ModInteractive> for FormatModInteractive {
    fn fmt_fields(&self, item: &ModInteractive, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item.range)])
    }
}
