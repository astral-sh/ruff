use ruff_formatter::write;
use ruff_python_ast::ModModule;
use ruff_python_trivia::lines_after;
use ruff_text_size::Ranged;

use crate::FormatNodeRule;
use crate::prelude::*;
use crate::statement::suite::SuiteKind;

#[derive(Default)]
pub struct FormatModModule;

impl FormatNodeRule<ruff_python_ast::kind::ModModule> for FormatModModule {
    fn fmt_fields(&self, item: ModModule<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let range = item.range();
        let body = item.body();

        if body.is_empty() {
            // Only preserve an empty line if the source contains an empty line too.
            if !f.context().comments().has_leading(item)
                && lines_after(range.start(), f.context().source()) != 0
            {
                empty_line().fmt(f)
            } else {
                Ok(())
            }
        } else {
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
}
