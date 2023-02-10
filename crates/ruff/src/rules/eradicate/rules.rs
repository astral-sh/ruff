use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Location;

use super::detection::comment_contains_code;
use crate::ast::types::Range;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::settings::{flags, Settings};
use crate::source_code::Locator;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    /// ### What it does
    /// Checks for commented-out Python code.
    ///
    /// ### Why is this bad?
    /// Commented-out code is dead code, and is often included inadvertently.
    /// It should be removed.
    ///
    /// ### Example
    /// ```python
    /// # print('foo')
    /// ```
    pub struct CommentedOutCode;
);
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
pub fn commented_out_code(
    locator: &Locator,
    start: Location,
    end: Location,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Option<Diagnostic> {
    let location = Location::new(start.row(), 0);
    let end_location = Location::new(end.row() + 1, 0);
    let line = locator.slice_source_code_range(&Range::new(location, end_location));

    // Verify that the comment is on its own line, and that it contains code.
    if is_standalone_comment(line) && comment_contains_code(line, &settings.task_tags[..]) {
        let mut diagnostic = Diagnostic::new(CommentedOutCode, Range::new(start, end));
        if matches!(autofix, flags::Autofix::Enabled)
            && settings.rules.should_fix(&Rule::CommentedOutCode)
        {
            diagnostic.amend(Fix::deletion(location, end_location));
        }
        Some(diagnostic)
    } else {
        None
    }
}
