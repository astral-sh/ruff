use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_source_file::Locator;

use crate::settings::LinterSettings;

use super::super::detection::comment_contains_code;

/// ## What it does
/// Checks for commented-out Python code.
///
/// ## Why is this bad?
/// Commented-out code is dead code, and is often included inadvertently.
/// It should be removed.
///
/// ## Known problems
/// Prone to false positives when checking comments that resemble Python code,
/// but are not actually Python code ([#4845]).
///
/// ## Example
/// ```python
/// # print("Hello, world!")
/// ```
///
/// ## Options
/// - `task-tags`
///
/// [#4845]: https://github.com/astral-sh/ruff/issues/4845
#[violation]
pub struct CommentedOutCode;

impl AlwaysFixableViolation for CommentedOutCode {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Found commented-out code")
    }

    fn fix_title(&self) -> String {
        "Remove commented-out code".to_string()
    }
}

fn is_standalone_comment(line: &str) -> bool {
    for char in line.chars() {
        if char == '#' {
            return true;
        } else if !char.is_whitespace() {
            return false;
        }
    }
    unreachable!("Comment should contain '#' character")
}

/// ERA001
pub(crate) fn commented_out_code(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    indexer: &Indexer,
    settings: &LinterSettings,
) {
    for range in indexer.comment_ranges() {
        let line = locator.full_lines(*range);

        // Verify that the comment is on its own line, and that it contains code.
        if is_standalone_comment(line) && comment_contains_code(line, &settings.task_tags[..]) {
            let mut diagnostic = Diagnostic::new(CommentedOutCode, *range);

            diagnostic.set_fix(Fix::display_edit(Edit::range_deletion(
                locator.full_lines_range(*range),
            )));
            diagnostics.push(diagnostic);
        }
    }
}
