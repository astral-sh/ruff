use ruff_formatter::write;
use ruff_python_ast::ModModule;
use ruff_python_trivia::lines_after;
use ruff_text_size::Ranged;

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
            // Check if the original source ends without a newline and if we're in a suppressed region
            let should_add_trailing_newline = {
                let source = f.context().source();
                let source_ends_without_newline = !source.ends_with('\n');

                // Check if we're currently in a suppressed region by scanning all comments
                let is_in_suppressed_region = {
                    let comment_ranges = f.context().comments().ranges();
                    let mut in_suppression = false;

                    // Get all comments in the file and check suppression state
                    for comment_range in comment_ranges.iter() {
                        let comment_text = &source[comment_range.range()];
                        if let Some(suppression_kind) =
                            ruff_python_trivia::SuppressionKind::from_comment(comment_text)
                        {
                            match suppression_kind {
                                ruff_python_trivia::SuppressionKind::Off => in_suppression = true,
                                ruff_python_trivia::SuppressionKind::On => in_suppression = false,
                                ruff_python_trivia::SuppressionKind::Skip => {} // Skip doesn't affect suppression state
                            }
                        }
                    }

                    in_suppression
                };

                !(source_ends_without_newline && is_in_suppressed_region)
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
