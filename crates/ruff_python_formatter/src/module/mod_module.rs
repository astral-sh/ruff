use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::hard_line_break;
use ruff_formatter::{write, Buffer, FormatResult};

use rustpython_parser::ast::ModModule;

#[derive(Default)]
pub struct FormatModModule;

impl FormatNodeRule<ModModule> for FormatModModule {
    fn fmt_fields(&self, item: &ModModule, f: &mut PyFormatter) -> FormatResult<()> {
        for stmt in &item.body {
            write!(f, [stmt.format(), hard_line_break()])?;
        }
        Ok(())
    }
}
