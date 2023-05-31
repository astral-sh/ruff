use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::Keyword;

#[derive(Default)]
pub(crate) struct FormatKeyword;

impl FormatNodeRule<Keyword> for FormatKeyword {
    fn fmt_fields(&self, _item: &Keyword, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
