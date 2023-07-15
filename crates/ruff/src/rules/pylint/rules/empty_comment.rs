use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::{Indexer, Locator};

use crate::registry::Rule;
use crate::settings::Settings;

/// ## What it does
/// Checks for useless empty comments.
///
/// ## Why is this bad?
/// It is not an actual comment.
///
/// ## Example
/// ```python
/// print(1)  #
/// ```
///
/// Use instead:
/// ```python
/// print(1)
/// ```
#[violation]
pub struct EmptyComment;

impl AlwaysAutofixableViolation for EmptyComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Line with empty comment")
    }

    fn autofix_title(&self) -> String {
        "Remove empty comment".to_string()
    }
}

fn comment_part_of_string(line: &str, idx: usize) -> bool {
    (line.get(..idx).unwrap().matches('"').count() % 2 == 1
        && line.get(idx + 1..).unwrap().matches('"').count() % 2 == 1)
        || (line.get(..idx).unwrap().matches('\'').count() % 2 == 1
            && line.get(idx + 1..).unwrap().matches('\'').count() % 2 == 1)
}

fn is_line_commented(line: &str) -> bool {
    if let Some(idx) = line.find('#') {
        if comment_part_of_string(line, idx) {
            is_line_commented(&format!(
                "{}{}",
                line.get(..idx).unwrap(),
                line.get(idx + 1..).unwrap()
            ))
        } else {
            true
        }
    } else {
        false
    }
}

/// R2044
pub(crate) fn empty_comment(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    indexer: &Indexer,
    settings: &Settings,
) {
    for range in indexer.comment_ranges() {
        let line = locator.full_lines(*range);
        let trimmed_line = line.trim_end();
        if !trimmed_line.ends_with('#') {
            continue;
        }

        if let Some(leftover) = trimmed_line.strip_suffix('#') {
            if is_line_commented(leftover) {
                continue;
            }

            let mut diagnostic = Diagnostic::new(EmptyComment, *range);
            if settings.rules.should_fix(Rule::EmptyComment) {
                diagnostic.set_fix(Fix::automatic(Edit::range_deletion(diagnostic.range())));
            }
            diagnostics.push(diagnostic);
        }
    }
}
