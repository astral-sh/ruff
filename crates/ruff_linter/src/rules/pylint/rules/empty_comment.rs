use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::is_python_whitespace;
use ruff_source_file::newlines::Line;
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
pub(crate) fn empty_comment(line: &Line) -> Option<Diagnostic> {
    let first_hash_col = u32::try_from(
        line.as_str()
            .char_indices()
            .find(|&(_, char)| char == '#')? // indicates the line does not contain a comment
            .0,
    )
    .unwrap(); // SAFETY: already know line is valid and all TextSize indices are u32

    let last_non_whitespace_col = u32::try_from(
        line.as_str()
            .char_indices()
            .rev()
            .find(|&(_, char)| !is_python_whitespace(char))
            .unwrap() // SAFETY: already verified at least one '#' char is in the line
            .0,
    )
    .unwrap(); // SAFETY: already know line is valid and all TextSize indices are u32

    if last_non_whitespace_col != first_hash_col {
        return None; // the comment is not empty
    }

    let deletion_start_col = match line
        .as_str()
        .char_indices()
        .rev()
        .find(|&(_, c)| !is_python_whitespace(c) && c != '#')
    {
        Some((last_non_whitespace_non_comment_col, _)) => {
            // SAFETY: (last_non_whitespace_non_comment_col + 1) <= u32::MAX because last_non_whitespace_col <= u32::MAX
            // and last_non_whitespace_non_comment_col < last_non_whitespace_col
            line.start()
                + TextSize::new(u32::try_from(last_non_whitespace_non_comment_col + 1).unwrap())
        }
        None => line.start(),
    };

    Some(
        Diagnostic::new(
            EmptyComment,
            TextRange::new(line.start() + TextSize::new(first_hash_col), line.end()),
        )
        .with_fix(Fix::safe_edit(Edit::deletion(
            deletion_start_col,
            line.end(),
        ))),
    )
}
