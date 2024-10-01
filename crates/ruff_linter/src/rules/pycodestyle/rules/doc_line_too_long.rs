use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::CommentRanges;
use ruff_source_file::Line;

use crate::rules::pycodestyle::overlong::Overlong;
use crate::settings::LinterSettings;

/// ## What it does
/// Checks for doc lines that exceed the specified maximum character length.
///
/// ## Why is this bad?
/// For flowing long blocks of text (docstrings or comments), overlong lines
/// can hurt readability. [PEP 8], for example, recommends that such lines be
/// limited to 72 characters, while this rule enforces the limit specified by
/// the [`lint.pycodestyle.max-doc-length`] setting. (If no value is provided, this
/// rule will be ignored, even if it's added to your `--select` list.)
///
/// In the context of this rule, a "doc line" is defined as a line consisting
/// of either a standalone comment or a standalone string, like a docstring.
///
/// In the interest of pragmatism, this rule makes a few exceptions when
/// determining whether a line is overlong. Namely, it:
///
/// 1. Ignores lines that consist of a single "word" (i.e., without any
///    whitespace between its characters).
/// 2. Ignores lines that end with a URL, as long as the URL starts before
///    the line-length threshold.
/// 3. Ignores line that end with a pragma comment (e.g., `# type: ignore`
///    or `# noqa`), as long as the pragma comment starts before the
///    line-length threshold. That is, a line will not be flagged as
///    overlong if a pragma comment _causes_ it to exceed the line length.
///    (This behavior aligns with that of the Ruff formatter.)
///
/// If [`lint.pycodestyle.ignore-overlong-task-comments`] is `true`, this rule will
/// also ignore comments that start with any of the specified [`lint.task-tags`]
/// (e.g., `# TODO:`).
///
/// ## Example
/// ```python
/// def function(x):
///     """Lorem ipsum dolor sit amet, consectetur adipiscing elit. Duis auctor purus ut ex fermentum, at maximus est hendrerit."""
/// ```
///
/// Use instead:
/// ```python
/// def function(x):
///     """
///     Lorem ipsum dolor sit amet, consectetur adipiscing elit.
///     Duis auctor purus ut ex fermentum, at maximus est hendrerit.
///     """
/// ```
///
/// ## Error suppression
/// Hint: when suppressing `W505` errors within multi-line strings (like
/// docstrings), the `noqa` directive should come at the end of the string
/// (after the closing triple quote), and will apply to the entire string, like
/// so:
///
/// ```python
/// """Lorem ipsum dolor sit amet.
///
/// Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.
/// """  # noqa: W505
/// ```
///
/// ## Options
/// - `lint.task-tags`
/// - `lint.pycodestyle.max-doc-length`
/// - `lint.pycodestyle.ignore-overlong-task-comments`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#maximum-line-length
#[violation]
pub struct DocLineTooLong(usize, usize);

impl Violation for DocLineTooLong {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DocLineTooLong(width, limit) = self;
        format!("Doc line too long ({width} > {limit})")
    }
}

/// W505
pub(crate) fn doc_line_too_long(
    line: &Line,
    comment_ranges: &CommentRanges,
    settings: &LinterSettings,
) -> Option<Diagnostic> {
    let limit = settings.pycodestyle.max_doc_length?;
    Overlong::try_from_line(
        line,
        comment_ranges,
        limit,
        if settings.pycodestyle.ignore_overlong_task_comments {
            &settings.task_tags
        } else {
            &[]
        },
        settings.tab_size,
    )
    .map(|overlong| {
        Diagnostic::new(
            DocLineTooLong(overlong.width(), limit.value() as usize),
            overlong.range(),
        )
    })
}
