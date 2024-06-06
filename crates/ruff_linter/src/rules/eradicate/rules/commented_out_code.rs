use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::CommentRanges;
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
/// - `lint.task-tags`
///
/// [#4845]: https://github.com/astral-sh/ruff/issues/4845
#[violation]
pub struct CommentedOutCode;

impl Violation for CommentedOutCode {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Found commented-out code")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Remove commented-out code"))
    }
}

/// ERA001
pub(crate) fn commented_out_code(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    settings: &LinterSettings,
) {
    // Skip comments within `/// script` tags.
    let mut in_script_tag = false;

    // Iterate over all comments in the document.
    for range in comment_ranges {
        let line = locator.lines(*range);

        // Detect `/// script` tags.
        if in_script_tag {
            if is_script_tag_end(line) {
                in_script_tag = false;
            }
        } else {
            if is_script_tag_start(line) {
                in_script_tag = true;
            }
        }

        // Skip comments within `/// script` tags.
        if in_script_tag {
            continue;
        }

        // Verify that the comment is on its own line, and that it contains code.
        if is_own_line_comment(line) && comment_contains_code(line, &settings.task_tags[..]) {
            let mut diagnostic = Diagnostic::new(CommentedOutCode, *range);
            diagnostic.set_fix(Fix::display_only_edit(Edit::range_deletion(
                locator.full_lines_range(*range),
            )));
            diagnostics.push(diagnostic);
        }
    }
}

/// Returns `true` if line contains an own-line comment.
fn is_own_line_comment(line: &str) -> bool {
    for char in line.chars() {
        if char == '#' {
            return true;
        }
        if !char.is_whitespace() {
            return false;
        }
    }
    unreachable!("Comment should contain '#' character")
}

/// Returns `true` if the line appears to start a script tag.
///
/// See: <https://peps.python.org/pep-0723/>
fn is_script_tag_start(line: &str) -> bool {
    line == "# /// script"
}

/// Returns `true` if the line appears to start a script tag.
///
/// See: <https://peps.python.org/pep-0723/>
fn is_script_tag_end(line: &str) -> bool {
    line == "# ///"
}
