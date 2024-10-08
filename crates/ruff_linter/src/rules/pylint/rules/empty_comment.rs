use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::{is_python_whitespace, CommentRanges};
use ruff_source_file::Locator;
use ruff_text_size::{TextRange, TextSize};

/// ## What it does
/// Checks for a # symbol appearing on a line not followed by an actual comment.
///
/// ## Why is this bad?
/// Empty comments don't provide any clarity to the code, and just add clutter.
/// Either add a comment or delete the empty comment.
///
/// ## Example
/// ```python
/// class Foo:  #
///     pass
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     pass
/// ```
///
/// ## References
/// - [Pylint documentation](https://pylint.pycqa.org/en/latest/user_guide/messages/refactor/empty-comment.html)
#[violation]
pub struct EmptyComment;

impl Violation for EmptyComment {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Line with empty comment")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Delete the empty comment"))
    }
}

/// PLR2044
pub(crate) fn empty_comments(
    diagnostics: &mut Vec<Diagnostic>,
    comment_ranges: &CommentRanges,
    locator: &Locator,
) {
    let block_comments = comment_ranges.block_comments(locator);

    for range in comment_ranges {
        // Ignore comments that are part of multi-line "comment blocks".
        if block_comments.binary_search(&range.start()).is_ok() {
            continue;
        }

        // If the line contains an empty comment, add a diagnostic.
        if let Some(diagnostic) = empty_comment(range, locator) {
            diagnostics.push(diagnostic);
        }
    }
}

/// Return a [`Diagnostic`] if the comment at the given [`TextRange`] is empty.
fn empty_comment(range: TextRange, locator: &Locator) -> Option<Diagnostic> {
    // Check: is the comment empty?
    if !locator
        .slice(range)
        .chars()
        .skip(1)
        .all(is_python_whitespace)
    {
        return None;
    }

    // Find the location of the `#`.
    let first_hash_col = range.start();

    // Find the start of the line.
    let line = locator.line_range(first_hash_col);

    // Find the last character in the line that precedes the comment, if any.
    let deletion_start_col = locator
        .slice(TextRange::new(line.start(), first_hash_col))
        .char_indices()
        .rev()
        .find_map(|(index, char)| {
            if is_python_whitespace(char) || char == '#' {
                None
            } else {
                // SAFETY: <= first_hash_col
                Some(TextSize::try_from(index + char.len_utf8()).unwrap())
            }
        });

    Some(
        Diagnostic::new(EmptyComment, TextRange::new(first_hash_col, line.end())).with_fix(
            Fix::safe_edit(if let Some(deletion_start_col) = deletion_start_col {
                Edit::deletion(line.start() + deletion_start_col, line.end())
            } else {
                Edit::range_deletion(locator.full_line_range(first_hash_col))
            }),
        ),
    )
}
