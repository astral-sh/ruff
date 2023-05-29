use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::Alias;

#[derive(Default)]
pub struct FormatAlias;

impl FormatNodeRule<Alias> for FormatAlias {
    fn fmt_fields(&self, _item: &Alias, _f: &mut PyFormatter) -> FormatResult<()> {
        todo!()
    }
}
