use anyhow::{bail, Result};
use log::error;
use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;

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
    range: TextRange,
    autofix: bool,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    let text = locator.slice(range);

    // Determine whether the string is single- or triple-quoted.
    let Ok(quote) = extract_quote(text) else {
        error!("Unable to find quotation mark for string token");
        return diagnostics;
    };
    let quote_pos = text.find(quote).unwrap();
    let prefix = text[..quote_pos].to_lowercase();
    let body = &text[quote_pos + quote.len()..text.len() - quote.len()];

    if !prefix.contains('r') {
        let start_offset =
            range.start() + TextSize::try_from(quote_pos).unwrap() + quote.text_len();

        let mut chars_iter = body.char_indices().peekable();

        while let Some((i, c)) = chars_iter.next() {
            if c != '\\' {
                continue;
            }

            // If the previous character was also a backslash, skip.
            if i > 0 && body.as_bytes()[i - 1] == b'\\' {
                continue;
            }

            // If we're at the end of the file, skip.
            let Some((_, next_char)) = chars_iter.peek() else {
                continue;
            };

            // If we're at the end of the line, skip
            if matches!(next_char, '\n' | '\r') {
                continue;
            }

            // If the next character is a valid escape sequence, skip.
            if VALID_ESCAPE_SEQUENCES.contains(next_char) {
                continue;
            }

            let location = start_offset + TextSize::try_from(i).unwrap();
            let range = TextRange::at(location, next_char.text_len() + TextSize::from(1));
            let mut diagnostic = Diagnostic::new(InvalidEscapeSequence(*next_char), range);
            if autofix {
                diagnostic.set_fix(Edit::insertion(
                    r"\".to_string(),
                    range.start() + TextSize::from(1),
                ));
            }
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}
