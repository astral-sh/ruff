use crate::statement::suite::SuiteLevel;
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::hard_line_break;
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ModModule;

#[derive(Default)]
pub struct FormatModModule;

impl FormatNodeRule<ModModule> for FormatModModule {
    fn fmt_fields(&self, item: &ModModule, f: &mut PyFormatter) -> FormatResult<()> {
        let ModModule {
            range: _,
            body,
            type_ignores,
        } = item;
        // https://docs.python.org/3/library/ast.html#ast-helpers
        debug_assert!(type_ignores.is_empty());
        write!(
            f,
            [
                body.format().with_options(SuiteLevel::TopLevel),
                // Trailing newline at the end of the file
                hard_line_break()
            ]
        )
    }
}
