use crate::statement::suite::SuiteLevel;
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::hard_line_break;
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::ModModule;

#[derive(Default)]
pub struct FormatModModule;

impl FormatNodeRule<ModModule> for FormatModModule {
    fn fmt_fields(&self, item: &ModModule, f: &mut PyFormatter) -> FormatResult<()> {
        let ModModule { range: _, body } = item;
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
