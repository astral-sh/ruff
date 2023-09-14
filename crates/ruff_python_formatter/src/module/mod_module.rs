use ruff_formatter::prelude::hard_line_break;
use ruff_formatter::{Buffer, FormatResult};
use ruff_python_ast::ModModule;

use crate::comments::{trailing_comments, SourceComment};
use crate::statement::suite::SuiteKind;
use crate::{write, AsFormat, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatModModule;

impl FormatNodeRule<ModModule> for FormatModModule {
    fn fmt_fields(&self, item: &ModModule, f: &mut PyFormatter) -> FormatResult<()> {
        let ModModule { range: _, body } = item;
        let comments = f.context().comments().clone();

        write!(
            f,
            [
                body.format().with_options(SuiteKind::TopLevel),
                trailing_comments(comments.dangling(item)),
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
