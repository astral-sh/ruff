use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::Line;

use crate::rules::pycodestyle::helpers::is_overlong;
use crate::settings::Settings;

/// ## What it does
/// Checks for doc lines that exceed the specified maximum character length.
///
/// ## Why is this bad?
/// For flowing long blocks of text (docstrings or comments), overlong lines
/// can hurt readability. [PEP 8], for example, recommends that such lines be
/// limited to 72 characters, while this rule enforces the limit specified by
/// the [`pycodestyle.max-doc-length`] setting. (If no value is provided, this
/// rule will be ignored, even if it's added to your `--select` list.)
///
/// In the context of this rule, a "doc line" is defined as a line consisting
/// of either a standalone comment or a standalone string, like a docstring.
///
/// In the interest of pragmatism, this rule makes a few exceptions when
/// determining whether a line is overlong. Namely, it ignores lines that
/// consist of a single "word" (i.e., without any whitespace between its
/// characters), and lines that end with a URL (as long as the URL starts
/// before the line-length threshold).
///
/// If [`pycodestyle.ignore-overlong-task-comments`] is `true`, this rule will
/// also ignore comments that start with any of the specified [`task-tags`]
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
/// ## Options
/// - `task-tags`
/// - `pycodestyle.max-doc-length`
/// - `pycodestyle.ignore-overlong-task-comments`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#maximum-line-length
#[violation]
pub struct DocLineTooLong(pub usize, pub usize);

impl Violation for DocLineTooLong {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DocLineTooLong(width, limit) = self;
        format!("Doc line too long ({width} > {limit} characters)")
    }
}

/// W505
pub(crate) fn doc_line_too_long(line: &Line, settings: &Settings) -> Option<Diagnostic> {
    let Some(limit) = settings.pycodestyle.max_doc_length else {
        return None;
    };

    is_overlong(
        line,
        limit,
        settings.pycodestyle.ignore_overlong_task_comments,
        &settings.task_tags,
        settings.tab_size,
    )
    .map(|overlong| {
        Diagnostic::new(
            DocLineTooLong(overlong.width(), limit.value() as usize),
            overlong.range(),
        )
    })
}
