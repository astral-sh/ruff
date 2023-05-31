use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use rustpython_parser::ast::ModModule;

#[derive(Default)]
pub(crate) struct FormatModModule;

impl FormatNodeRule<ModModule> for FormatModModule {
    fn fmt_fields(&self, _item: &ModModule, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
