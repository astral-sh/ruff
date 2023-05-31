use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ModInteractive;

#[derive(Default)]
pub(crate) struct FormatModInteractive;

impl FormatNodeRule<ModInteractive> for FormatModInteractive {
    fn fmt_fields(&self, _item: &ModInteractive, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
