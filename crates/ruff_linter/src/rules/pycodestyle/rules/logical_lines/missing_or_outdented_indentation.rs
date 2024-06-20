use super::{LogicalLine, LogicalLineToken};
use crate::checkers::logical_lines::LogicalLinesContext;
use crate::line_width::IndentWidth;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_python_parser::TokenKind;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::rules::pycodestyle::helpers::expand_indent;

/// ## What it does
/// Checks for continuation lines without enough indentation.
///
/// ## Why is this bad?
/// This makes distinguishing continuation lines more difficult.
///
/// ## Example
/// ```python
/// print("Python", (
/// "Rules"))
/// ```
///
/// Use instead:
/// ```python
/// print("Python", (
///     "Rules"))
/// ```
///
/// [PEP 8]: https://www.python.org/dev/peps/pep-0008/#indentation
#[violation]
pub struct MissingOrOutdentedIndentation;

impl Violation for MissingOrOutdentedIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line missing indentation or outdented.")
    }
}

/// E122
pub(crate) fn missing_or_outdented_indentation(
    line: &LogicalLine,
    indent_level: usize,
    indent_width: IndentWidth,
    locator: &Locator,
    indexer: &Indexer,
    context: &mut LogicalLinesContext,
) {
    if line.tokens().len() <= 1 {
        return;
    }

    let first_token = line.first_token().unwrap();
    let mut next_continuation = continuation_line_end(locator, indexer, first_token);

    let tab_size = indent_width.as_usize();
    let mut indentation = indent_level;
    // Start by increasing indent on any continuation line
    let mut desired_indentation = indentation + tab_size;
    let mut indentation_changed = true;
    let mut indentation_stack: std::vec::Vec<usize> = Vec::new();
    let mut fstrings = 0u32;
    let mut newline = false;

    for token in line.tokens() {
        // If continuation line
        if newline || (next_continuation.is_some() && token.start() >= next_continuation.unwrap()) {
            newline = false;
            // Reset and calculate current indentation
            indentation_changed = false;
            let range = TextRange::new(locator.line_start(token.start()), token.start());
            indentation = expand_indent(locator.slice(range), indent_width);

            // Calculate correct indentation
            let correct_indentation = if token_is_closing(token) {
                // If first token is a closing bracket or fstring-end
                // then the correct indentation is the one on top of the stack
                // unless we are back at the starting indentation in which case
                // the initial indentation is correct.
                if desired_indentation == indent_level + tab_size {
                    indent_level
                } else {
                    *indentation_stack
                        .last()
                        .expect("Closing brackets should always be preceded by opening brackets")
                }
            } else {
                desired_indentation
            };

            if fstrings == 0 && indentation < correct_indentation {
                let diagnostic = Diagnostic::new(MissingOrOutdentedIndentation, range);
                context.push_diagnostic(diagnostic);
            }

            if next_continuation.is_some() && token.start() >= next_continuation.unwrap() {
                next_continuation = continuation_line_end(locator, indexer, token);
            }
        }

        match token.kind() {
            TokenKind::FStringStart => fstrings += 1,
            TokenKind::FStringEnd => fstrings = fstrings.saturating_sub(1),
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace if fstrings == 0 => {
                // Store indent to return to once bracket closes
                indentation_stack.push(desired_indentation);
                // Only increase the indent once per continuation line
                if !indentation_changed {
                    desired_indentation += tab_size;
                    indentation_changed = true;
                }
            }
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace if fstrings == 0 => {
                // Return to previous indent
                desired_indentation = indentation_stack
                    .pop()
                    .expect("Closing brackets should always be preceded by opening brackets");
                indentation_changed = true;
            }
            TokenKind::Newline | TokenKind::NonLogicalNewline => newline = true,
            _ => {}
        }
    }
}

fn token_is_closing(token: &LogicalLineToken) -> bool {
    matches!(
        token.kind,
        TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
    )
}

fn continuation_line_end(
    locator: &Locator,
    indexer: &Indexer,
    token: &LogicalLineToken,
) -> Option<TextSize> {
    let continuation_lines = indexer.continuation_line_starts();
    let continuation_line_index = continuation_lines
        .binary_search(&token.start())
        .unwrap_or_else(|err_index| err_index);
    let continuation_line_start = continuation_lines.get(continuation_line_index)?;
    Some(locator.full_line_end(*continuation_line_start))
}
