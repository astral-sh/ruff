use ruff_formatter::write;
use ruff_python_ast::ModModule;
use ruff_python_trivia::lines_after;

use crate::FormatNodeRule;
use crate::prelude::*;
use crate::statement::suite::SuiteKind;

#[derive(Default)]
pub struct FormatModModule;

impl FormatNodeRule<ModModule> for FormatModModule {
    fn fmt_fields(&self, item: &ModModule, f: &mut PyFormatter) -> FormatResult<()> {
        let ModModule {
            range,
            body,
            node_index: _,
        } = item;

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
            // Check if the original source ends without a newline and has fmt: off
            let should_add_trailing_newline = {
                let source = f.context().source();
                let source_ends_without_newline = !source.ends_with('\n');
                let has_fmt_off = source.contains("# fmt: off");
                !(source_ends_without_newline && has_fmt_off)
            };

            if should_add_trailing_newline {
                write!(
                    f,
                    [
                        body.format().with_options(SuiteKind::TopLevel),
                        // Trailing newline at the end of the file
                        hard_line_break()
                    ]
                )
            } else {
                // Don't add trailing newline if we're in a suppressed region
                body.format().with_options(SuiteKind::TopLevel).fmt(f)
            }
        }
    }
}
