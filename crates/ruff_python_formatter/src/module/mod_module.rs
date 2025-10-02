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

        let source = f.context().source();

        if body.is_empty() {
            let comments = f.context().comments();
            // Only preserve an empty line if the source contains an empty line too.
            if !comments.has_leading(item) && lines_after(range.start(), source) != 0 {
                empty_line().fmt(f)
            } else {
                Ok(())
            }
        } else {
            body.format().with_options(SuiteKind::TopLevel).fmt(f)?;

            if source.ends_with('\n') || !has_unclosed_fmt_off(body, f.context()) {
                hard_line_break().fmt(f)?;
            }

            Ok(())
        }
    }
}

/// Determines if there's an unclosed fmt:off suppression at the end of the module
fn has_unclosed_fmt_off(body: &[ruff_python_ast::Stmt], context: &PyFormatContext) -> bool {
    let comments = context.comments();
    let source = context.source();

    // Determine if module-level suppression is active at EOF by
    // iterating comments attached to top-level statements.
    let mut module_suppressed_at_eof = false;
    for statement in body {
        for comment in comments.leading_trailing(statement) {
            if comment.is_suppression_off_comment(source) {
                module_suppressed_at_eof = true;
            } else if comment.is_suppression_on_comment(source) {
                module_suppressed_at_eof = false;
            }
        }
    }

    module_suppressed_at_eof
}
