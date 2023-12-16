use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use ruff_python_trivia::PythonWhitespace;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::checkers::logical_lines::LogicalLinesContext;
use crate::rules::pycodestyle::rules::logical_lines::LogicalLine;

/// ## What it does
/// Checks if inline comments are separated by at least two spaces.
///
/// ## Why is this bad?
/// An inline comment is a comment on the same line as a statement.
///
/// Per PEP8, inline comments should be separated by at least two spaces from
/// the preceding statement.
///
/// ## Example
/// ```python
/// x = x + 1 # Increment x
/// ```
///
/// Use instead:
/// ```python
/// x = x + 1  # Increment x
/// x = x + 1    # Increment x
/// ```
#[violation]
pub struct TooFewSpacesBeforeInlineComment;

impl AlwaysFixableViolation for TooFewSpacesBeforeInlineComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Insert at least two spaces before an inline comment")
    }

    fn fix_title(&self) -> String {
        format!("Insert spaces")
    }
}

/// ## What it does
/// Checks if one space is used after inline comments.
///
/// ## Why is this bad?
/// An inline comment is a comment on the same line as a statement.
///
/// Per PEP8, inline comments should start with a # and a single space.
///
/// ## Example
/// ```python
/// x = x + 1  #Increment x
/// x = x + 1  #  Increment x
/// x = x + 1  # \xa0Increment x
/// ```
///
/// Use instead:
/// ```python
/// x = x + 1  # Increment x
/// x = x + 1    # Increment x
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#comments
#[violation]
pub struct NoSpaceAfterInlineComment;

impl AlwaysFixableViolation for NoSpaceAfterInlineComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Inline comment should start with `# `")
    }

    fn fix_title(&self) -> String {
        format!("Format space")
    }
}

/// ## What it does
/// Checks if one space is used after block comments.
///
/// ## Why is this bad?
/// Per PEP8, "Block comments generally consist of one or more paragraphs built
/// out of complete sentences, with each sentence ending in a period."
///
/// Block comments should start with a # and a single space.
///
/// ## Example
/// ```python
/// #Block comment
/// ```
///
/// Use instead:
/// ```python
/// # Block comments:
/// #  - Block comment list
/// # \xa0- Block comment list
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#comments
#[violation]
pub struct NoSpaceAfterBlockComment;

impl AlwaysFixableViolation for NoSpaceAfterBlockComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Block comment should start with `# `")
    }

    fn fix_title(&self) -> String {
        format!("Format space")
    }
}

/// ## What it does
/// Checks if block comments start with a single "#".
///
/// ## Why is this bad?
/// Per PEP8, "Block comments generally consist of one or more paragraphs built
/// out of complete sentences, with each sentence ending in a period."
///
/// Each line of a block comment should start with a # and a single space.
///
/// ## Example
/// ```python
/// ### Block comment
/// ```
///
/// Use instead:
/// ```python
/// # Block comments:
/// #  - Block comment list
/// # \xa0- Block comment list
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#comments
#[violation]
pub struct MultipleLeadingHashesForBlockComment;

impl AlwaysFixableViolation for MultipleLeadingHashesForBlockComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Too many leading `#` before block comment")
    }

    fn fix_title(&self) -> String {
        format!("Remove leading `#`")
    }
}

/// E261, E262, E265, E266
pub(crate) fn whitespace_before_comment(
    line: &LogicalLine,
    locator: &Locator,
    context: &mut LogicalLinesContext,
) {
    let mut prev_end = TextSize::default();
    for token in line.tokens() {
        let kind = token.kind();

        if let TokenKind::Comment = kind {
            let range = token.range();

            let line_text = locator.slice(TextRange::new(
                locator.line_start(range.start()),
                range.start(),
            ));
            let token_text = locator.slice(range);

            let is_inline_comment = !line_text.trim_whitespace().is_empty();
            if is_inline_comment {
                if range.start() - prev_end < "  ".text_len() {
                    let mut diagnostic = Diagnostic::new(
                        TooFewSpacesBeforeInlineComment,
                        TextRange::new(prev_end, range.start()),
                    );
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        "  ".to_string(),
                        TextRange::new(prev_end, range.start()),
                    )));
                    context.push_diagnostic(diagnostic);
                }
            }

            // Split into the portion before and after the first space.
            let mut parts = token_text.splitn(2, ' ');
            let symbol = parts.next().unwrap_or("");
            let comment = parts.next().unwrap_or("");

            let bad_prefix = if symbol != "#" && symbol != "#:" {
                Some(symbol.trim_start_matches('#').chars().next().unwrap_or('#'))
            } else {
                None
            };

            if is_inline_comment {
                if bad_prefix.is_some() || comment.chars().next().is_some_and(char::is_whitespace) {
                    let mut diagnostic = Diagnostic::new(NoSpaceAfterInlineComment, range);
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        format_leading_space(token_text),
                        range,
                    )));
                    context.push_diagnostic(diagnostic);
                }
            } else if let Some(bad_prefix) = bad_prefix {
                if bad_prefix != '!' || !line.is_start_of_file() {
                    if bad_prefix != '#' {
                        let mut diagnostic = Diagnostic::new(NoSpaceAfterBlockComment, range);
                        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                            format_leading_space(token_text),
                            range,
                        )));
                        context.push_diagnostic(diagnostic);
                    } else if !comment.is_empty() {
                        let mut diagnostic =
                            Diagnostic::new(MultipleLeadingHashesForBlockComment, range);
                        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                            format_leading_hashes(token_text),
                            range,
                        )));
                        context.push_diagnostic(diagnostic);
                    }
                }
            }
        } else if !matches!(kind, TokenKind::NonLogicalNewline) {
            prev_end = token.end();
        }
    }
}

/// Format a comment to have a single space after the `#`.
fn format_leading_space(comment: &str) -> String {
    if let Some(rest) = comment.strip_prefix("#:") {
        format!("#: {}", rest.trim_start())
    } else {
        format!("# {}", comment.trim_start_matches('#').trim_start())
    }
}

/// Format a comment to strip multiple leading `#` characters.
fn format_leading_hashes(comment: &str) -> String {
    format!("# {}", comment.trim_start_matches('#').trim_start())
}
