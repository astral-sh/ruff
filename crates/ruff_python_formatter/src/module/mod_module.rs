use ruff_formatter::write;
use ruff_python_ast::ModModule;

use crate::comments::SourceComment;
use crate::prelude::*;
use crate::statement::suite::SuiteKind;
use crate::FormatNodeRule;

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

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled as part of `fmt_fields`
        Ok(())
    }
}
