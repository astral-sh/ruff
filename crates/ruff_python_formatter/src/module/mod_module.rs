use ruff_formatter::prelude::hard_line_break;
use ruff_formatter::write;
use ruff_python_ast::ModModule;

use crate::prelude::*;
use crate::statement::suite::SuiteKind;

#[derive(Default)]
pub struct FormatModModule;

impl FormatNodeRule<ModModule> for FormatModModule {
    fn fmt_fields(&self, item: &ModModule, f: &mut PyFormatter) -> FormatResult<()> {
        let ModModule { range: _, body } = item;
        write!(
            f,
            [
                body.format().with_options(SuiteKind::TopLevel),
                // Trailing newline at the end of the file
                hard_line_break()
            ]
        )
    }
}
