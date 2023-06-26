use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::str::{leading_quote, trailing_quote};

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
    locator: &Locator,
    range: TextRange,
    autofix: bool,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    let text = locator.slice(range);

    // Determine whether the string is single- or triple-quoted.
    let Some(leading_quote) = leading_quote(text) else {
        return diagnostics;
    };
    let Some(trailing_quote) = trailing_quote(text) else {
        return diagnostics;
    };
    let body = &text[leading_quote.len()..text.len() - trailing_quote.len()];

    if leading_quote.contains(['r', 'R']) {
        return diagnostics;
    }

    let start_offset = range.start() + TextSize::try_from(leading_quote.len()).unwrap();

    let mut chars_iter = body.char_indices().peekable();

    let mut contains_valid_escape_sequence = false;

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

        let location = start_offset + TextSize::try_from(i).unwrap();
        let range = TextRange::at(location, next_char.text_len() + TextSize::from(1));
        let diagnostic = Diagnostic::new(InvalidEscapeSequence(*next_char), range);
        diagnostics.push(diagnostic);
    }

    if autofix {
        if contains_valid_escape_sequence {
            // Escape with backslash.
            for diagnostic in &mut diagnostics {
                diagnostic.set_fix(Fix::automatic(Edit::insertion(
                    r"\".to_string(),
                    diagnostic.range().start() + TextSize::from(1),
                )));
            }
        } else {
            // Turn into raw string.
            for diagnostic in &mut diagnostics {
                // If necessary, add a space between any leading keyword (`return`, `yield`,
                // `assert`, etc.) and the string. For example, `return"foo"` is valid, but
                // `returnr"foo"` is not.
                let requires_space = locator
                    .slice(TextRange::up_to(range.start()))
                    .chars()
                    .last()
                    .map_or(false, |char| char.is_ascii_alphabetic());

                diagnostic.set_fix(Fix::automatic(Edit::insertion(
                    if requires_space {
                        " r".to_string()
                    } else {
                        "r".to_string()
                    },
                    range.start(),
                )));
            }
        }
    }

    diagnostics
}
