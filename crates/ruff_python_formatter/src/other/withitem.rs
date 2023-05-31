use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::Withitem;

#[derive(Default)]
pub(crate) struct FormatWithitem;

impl FormatNodeRule<Withitem> for FormatWithitem {
    fn fmt_fields(&self, _item: &Withitem, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
