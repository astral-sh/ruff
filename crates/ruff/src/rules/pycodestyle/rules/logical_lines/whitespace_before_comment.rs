use super::LogicalLineTokens;
use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::token_kind::TokenKind;
use ruff_python_ast::types::Range;
use rustpython_parser::ast::Location;

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
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#comments)
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
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#comments)
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
///
/// ```
///
/// Use instead:
/// ```python
/// # Block comments:
/// #  - Block comment list
/// # \xa0- Block comment list
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#comments)
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
    tokens: &LogicalLineTokens,
    locator: &Locator,
) -> Vec<(Range, DiagnosticKind)> {
    let mut diagnostics = vec![];
    let mut prev_end = Location::new(0, 0);
    for token in tokens {
        let kind = token.kind();

        if let TokenKind::Comment = kind {
            let (start, end) = token.range();
            let line = locator.slice(Range::new(
                Location::new(start.row(), 0),
                Location::new(start.row(), start.column()),
            ));

            let text = locator.slice(Range::new(start, end));

            let is_inline_comment = !line.trim().is_empty();
            if is_inline_comment {
                if prev_end.row() == start.row() && start.column() < prev_end.column() + 2 {
                    diagnostics.push((
                        Range::new(prev_end, start),
                        TooFewSpacesBeforeInlineComment.into(),
                    ));
                }
            }

            // Split into the portion before and after the first space.
            let mut parts = text.splitn(2, ' ');
            let symbol = parts.next().unwrap_or("");
            let comment = parts.next().unwrap_or("");

            let bad_prefix = if symbol != "#" && symbol != "#:" {
                Some(symbol.trim_start_matches('#').chars().next().unwrap_or('#'))
            } else {
                None
            };

            if is_inline_comment {
                if bad_prefix.is_some() || comment.chars().next().map_or(false, char::is_whitespace)
                {
                    diagnostics.push((Range::new(start, end), NoSpaceAfterInlineComment.into()));
                }
            } else if let Some(bad_prefix) = bad_prefix {
                if bad_prefix != '!' || start.row() > 1 {
                    if bad_prefix != '#' {
                        diagnostics.push((Range::new(start, end), NoSpaceAfterBlockComment.into()));
                    } else if !comment.is_empty() {
                        diagnostics.push((
                            Range::new(start, end),
                            MultipleLeadingHashesForBlockComment.into(),
                        ));
                    }
                }
            }
        } else if !matches!(kind, TokenKind::NonLogicalNewline) {
            prev_end = token.end();
        }
    }
    diagnostics
}
