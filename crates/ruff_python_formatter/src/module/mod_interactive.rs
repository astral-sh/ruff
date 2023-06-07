use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ModInteractive;

#[derive(Default)]
pub struct FormatModInteractive;

impl FormatNodeRule<ModInteractive> for FormatModInteractive {
    fn fmt_fields(&self, item: &ModInteractive, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
