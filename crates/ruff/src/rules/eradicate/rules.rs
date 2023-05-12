use ruff_text_size::TextRange;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;

use crate::registry::Rule;
use crate::settings::Settings;

use super::detection::comment_contains_code;

/// ## What it does
/// Checks for commented-out Python code.
///
/// ## Why is this bad?
/// Commented-out code is dead code, and is often included inadvertently.
/// It should be removed.
///
/// ## Example
/// ```python
/// # print('foo')
/// ```
#[violation]
pub struct CommentedOutCode;

impl AlwaysAutofixableViolation for CommentedOutCode {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Found commented-out code")
    }

    fn autofix_title(&self) -> String {
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
    locator: &Locator,
    range: TextRange,
    settings: &Settings,
) -> Option<Diagnostic> {
    let line = locator.full_lines(range);

    // Verify that the comment is on its own line, and that it contains code.
    if is_standalone_comment(line) && comment_contains_code(line, &settings.task_tags[..]) {
        let mut diagnostic = Diagnostic::new(CommentedOutCode, range);

        if settings.rules.should_fix(Rule::CommentedOutCode) {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_deletion(
                locator.full_lines_range(range),
            )));
        }
        Some(diagnostic)
    } else {
        None
    }
}
