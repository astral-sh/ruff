use ruff_diagnostics::Violation;
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

impl Violation for TooFewSpacesBeforeInlineComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Insert at least two spaces before an inline comment")
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

impl Violation for NoSpaceAfterInlineComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Inline comment should start with `# `")
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

impl Violation for NoSpaceAfterBlockComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Block comment should start with `# `")
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

impl Violation for MultipleLeadingHashesForBlockComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Too many leading `#` before block comment")
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
                    context.push(
                        TooFewSpacesBeforeInlineComment,
                        TextRange::new(prev_end, range.start()),
                    );
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
                    context.push(NoSpaceAfterInlineComment, range);
                }
            } else if let Some(bad_prefix) = bad_prefix {
                if bad_prefix != '!' || !line.is_start_of_file() {
                    if bad_prefix != '#' {
                        context.push(NoSpaceAfterBlockComment, range);
                    } else if !comment.is_empty() {
                        context.push(MultipleLeadingHashesForBlockComment, range);
                    }
                }
            }
        } else if !matches!(kind, TokenKind::NonLogicalNewline) {
            prev_end = token.end();
        }
    }
}
