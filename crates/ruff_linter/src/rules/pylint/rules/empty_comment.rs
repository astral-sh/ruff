use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_index::Indexer;
use ruff_python_trivia::{CommentRanges, is_python_whitespace};
use ruff_source_file::LineRanges;
use ruff_text_size::{TextRange, TextSize};

use crate::Locator;
use crate::checkers::ast::LintContext;
use crate::{Edit, Fix, FixAvailability, Violation};

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.5.0")]
pub(crate) struct EmptyComment;

impl Violation for EmptyComment {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Line with empty comment".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Delete the empty comment".to_string())
    }
}

/// PLR2044
pub(crate) fn empty_comments(
    context: &LintContext,
    comment_ranges: &CommentRanges,
    locator: &Locator,
    indexer: &Indexer,
) {
    let block_comments = comment_ranges.block_comments(locator.contents());

    for range in comment_ranges {
        // Ignore comments that are part of multi-line "comment blocks".
        if block_comments.binary_search(&range.start()).is_ok() {
            continue;
        }

        // If the line contains an empty comment, add a diagnostic.
        empty_comment(context, range, locator, indexer);
    }
}

/// Return a [`Diagnostic`] if the comment at the given [`TextRange`] is empty.
fn empty_comment(context: &LintContext, range: TextRange, locator: &Locator, indexer: &Indexer) {
    // Check: is the comment empty?
    if !locator
        .slice(range)
        .chars()
        .skip(1)
        .all(is_python_whitespace)
    {
        return;
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

    // If there is no character preceding the comment, this comment must be on its own physical line.
    // If there is a line preceding the empty comment's line, check if it ends in a line continuation character. (`\`)
    let is_on_same_logical_line = indexer
        .preceded_by_continuations(first_hash_col, locator.contents())
        .is_some();

    if let Some(mut diagnostic) = context
        .report_diagnostic_if_enabled(EmptyComment, TextRange::new(first_hash_col, line.end()))
    {
        diagnostic.set_fix(Fix::safe_edit(
            if let Some(deletion_start_col) = deletion_start_col {
                Edit::deletion(line.start() + deletion_start_col, line.end())
            } else if is_on_same_logical_line {
                Edit::deletion(first_hash_col, line.end())
            } else {
                Edit::range_deletion(locator.full_line_range(first_hash_col))
            },
        ));
    }
}
