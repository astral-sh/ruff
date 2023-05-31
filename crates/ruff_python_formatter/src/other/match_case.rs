use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::MatchCase;

#[derive(Default)]
pub(crate) struct FormatMatchCase;

impl FormatNodeRule<MatchCase> for FormatMatchCase {
    fn fmt_fields(&self, _item: &MatchCase, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
