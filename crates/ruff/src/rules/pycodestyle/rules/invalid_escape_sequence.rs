use memchr::memchr_iter;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::{leading_quote, trailing_quote};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::autofix::edits::pad_start;

/// ## What it does
/// Checks for invalid escape sequences.
///
/// ## Why is this bad?
/// Invalid escape sequences are deprecated in Python 3.6.
///
/// ## Example
/// ```python
/// regex = "\.png$"
/// ```
///
/// Use instead:
/// ```python
/// regex = r"\.png$"
/// ```
///
/// ## References
/// - [Python documentation: String and Bytes literals](https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals)
#[violation]
pub struct InvalidEscapeSequence(char);

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

/// W605
pub(crate) fn invalid_escape_sequence(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    range: TextRange,
    autofix: bool,
) {
    let text = locator.slice(range);

    // Determine whether the string is single- or triple-quoted.
    let Some(leading_quote) = leading_quote(text) else {
        return;
    };
    let Some(trailing_quote) = trailing_quote(text) else {
        return;
    };
    let body = &text[leading_quote.len()..text.len() - trailing_quote.len()];

    if leading_quote.contains(['r', 'R']) {
        return;
    }

    let mut contains_valid_escape_sequence = false;
    let mut invalid_escape_sequence = Vec::new();

    let mut prev = None;
    let bytes = body.as_bytes();
    for i in memchr_iter(b'\\', bytes) {
        // If the previous character was also a backslash, skip.
        if prev.is_some_and(|prev| prev == i - 1) {
            prev = None;
            continue;
        }

        prev = Some(i);

        let Some(next_char) = body[i + 1..].chars().next() else {
            // If we're at the end of the file, skip.
            continue;
        };

        // If we're at the end of line, skip.
        if matches!(next_char, '\n' | '\r') {
            continue;
        }

        // If the next character is a valid escape sequence, skip.
        // See: https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals.
        if matches!(
            next_char,
            '\n'
            | '\\'
            | '\''
            | '"'
            | 'a'
            | 'b'
            | 'f'
            | 'n'
            | 'r'
            | 't'
            | 'v'
            | '0'
            | '1'
            | '2'
            | '3'
            | '4'
            | '5'
            | '6'
            | '7'
            | 'x'
            // Escape sequences only recognized in string literals
            | 'N'
            | 'u'
            | 'U'
        ) {
            contains_valid_escape_sequence = true;
            continue;
        }

        let location = range.start() + leading_quote.text_len() + TextSize::try_from(i).unwrap();
        let range = TextRange::at(location, next_char.text_len() + TextSize::from(1));
        invalid_escape_sequence.push(Diagnostic::new(InvalidEscapeSequence(next_char), range));
    }

    if autofix {
        if contains_valid_escape_sequence {
            // Escape with backslash.
            for diagnostic in &mut invalid_escape_sequence {
                diagnostic.set_fix(Fix::automatic(Edit::insertion(
                    r"\".to_string(),
                    diagnostic.start() + TextSize::from(1),
                )));
            }
        } else {
            // Turn into raw string.
            for diagnostic in &mut invalid_escape_sequence {
                // If necessary, add a space between any leading keyword (`return`, `yield`,
                // `assert`, etc.) and the string. For example, `return"foo"` is valid, but
                // `returnr"foo"` is not.
                diagnostic.set_fix(Fix::automatic(Edit::insertion(
                    pad_start("r".to_string(), range.start(), locator),
                    range.start(),
                )));
            }
        }
    }

    diagnostics.extend(invalid_escape_sequence);
}
