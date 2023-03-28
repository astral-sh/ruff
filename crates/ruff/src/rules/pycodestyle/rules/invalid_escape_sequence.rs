use anyhow::{bail, Result};
use log::error;
use rustpython_parser::ast::Location;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

/// ## What it does
/// Checks for invalid escape sequences.
///
/// ## Why is this bad?
/// Invalid escape sequences are deprecated in Python 3.6.
///
/// ## Example
/// ```python
/// regex = '\.png$'
/// ```
///
/// Use instead:
/// ```python
/// regex = r'\.png$'
/// ```
#[violation]
pub struct InvalidEscapeSequence(pub char);

impl AlwaysAutofixableViolation for InvalidEscapeSequence {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidEscapeSequence(char) = self;
        format!("Invalid escape sequence: `\\{char}`")
    }

    fn autofix_title(&self) -> String {
        "Add backslash to escape sequence".to_string()
    }
}

// See: https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals
const VALID_ESCAPE_SEQUENCES: &[char; 23] = &[
    '\n', '\\', '\'', '"', 'a', 'b', 'f', 'n', 'r', 't', 'v', '0', '1', '2', '3', '4', '5', '6',
    '7', 'x', // Escape sequences only recognized in string literals
    'N', 'u', 'U',
];

/// Return the quotation markers used for a String token.
fn extract_quote(text: &str) -> Result<&str> {
    for quote in ["'''", "\"\"\"", "'", "\""] {
        if text.ends_with(quote) {
            return Ok(quote);
        }
    }

    bail!("Unable to find quotation mark for String token")
}

/// W605
pub fn invalid_escape_sequence(
    locator: &Locator,
    start: Location,
    end: Location,
    autofix: bool,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    let text = locator.slice(Range::new(start, end));

    // Determine whether the string is single- or triple-quoted.
    let Ok(quote) = extract_quote(text) else {
        error!("Unable to find quotation mark for string token");
        return diagnostics;
    };
    let quote_pos = text.find(quote).unwrap();
    let prefix = text[..quote_pos].to_lowercase();
    let body = &text[(quote_pos + quote.len())..(text.len() - quote.len())];

    if !prefix.contains('r') {
        for (row_offset, line) in body.universal_newlines().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            for col_offset in 0..chars.len() {
                if chars[col_offset] != '\\' {
                    continue;
                }

                // If the previous character was also a backslash, skip.
                if col_offset > 0 && chars[col_offset - 1] == '\\' {
                    continue;
                }

                // If we're at the end of the line, skip.
                if col_offset == chars.len() - 1 {
                    continue;
                }

                // If the next character is a valid escape sequence, skip.
                let next_char = chars[col_offset + 1];
                if VALID_ESCAPE_SEQUENCES.contains(&next_char) {
                    continue;
                }

                // Compute the location of the escape sequence by offsetting the location of the
                // string token by the characters we've seen thus far.
                let col = if row_offset == 0 {
                    start.column() + prefix.len() + quote.len() + col_offset
                } else {
                    col_offset
                };
                let location = Location::new(start.row() + row_offset, col);
                let end_location = Location::new(location.row(), location.column() + 2);
                let mut diagnostic = Diagnostic::new(
                    InvalidEscapeSequence(next_char),
                    Range::new(location, end_location),
                );
                if autofix {
                    diagnostic.set_fix(Edit::insertion(
                        r"\".to_string(),
                        Location::new(location.row(), location.column() + 1),
                    ));
                }
                diagnostics.push(diagnostic);
            }
        }
    }

    diagnostics
}
