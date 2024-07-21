use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

/// ## What it does
/// Checks for docstrings that include backslashes, but are not defined as
/// raw string literals.
///
/// ## Why is this bad?
/// In Python, backslashes are typically used to escape characters in strings.
/// In raw strings (those prefixed with an `r`), however, backslashes are
/// treated as literal characters.
///
/// [PEP 257](https://peps.python.org/pep-0257/#what-is-a-docstring) recommends
/// the use of raw strings (i.e., `r"""raw triple double quotes"""`) for
/// docstrings that include backslashes. The use of a raw string ensures that
/// any backslashes are treated as literal characters, and not as escape
/// sequences, which avoids confusion.
///
/// ## Example
/// ```python
/// def foobar():
///     """Docstring for foo\bar."""
///
///
/// foobar.__doc__  # "Docstring for foar."
/// ```
///
/// Use instead:
/// ```python
/// def foobar():
///     r"""Docstring for foo\bar."""
///
///
/// foobar.__doc__  # "Docstring for foo\bar."
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [Python documentation: String and Bytes literals](https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals)
#[violation]
pub struct EscapeSequenceInDocstring;

impl Violation for EscapeSequenceInDocstring {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"Use `r"""` if any backslashes in a docstring"#)
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!(r#"Add `r` prefix"#))
    }
}

/// D301
pub(crate) fn backslashes(checker: &mut Checker, docstring: &Docstring) {
    // Docstring is already raw.
    if docstring.leading_quote().contains(['r', 'R']) {
        return;
    }

    // Docstring contains at least one backslash.
    let body = docstring.body();
    let bytes = body.as_bytes();
    let mut offset = 0;
    while let Some(position) = memchr::memchr(b'\\', &bytes[offset..]) {
        if position + offset + 1 >= body.len() {
            break;
        }

        let after_escape = &body[position + offset + 1..];

        // End of Docstring.
        let Some(escaped_char) = &after_escape.chars().next() else {
            break;
        };

        if matches!(escaped_char, '"' | '\'') {
            // If the next three characters are equal to """, it indicates an escaped docstring pattern.
            if after_escape.starts_with("\"\"\"") || after_escape.starts_with("\'\'\'") {
                offset += position + 3;
                continue;
            }
            // If the next three characters are equal to "\"\", it indicates an escaped docstring pattern.
            if after_escape.starts_with("\"\\\"\\\"") || after_escape.starts_with("\'\\\'\\\'") {
                offset += position + 5;
                continue;
            }
        }

        offset += position + escaped_char.len_utf8();

        // Only allow continuations (backslashes followed by newlines) and Unicode escapes.
        if !matches!(*escaped_char, '\r' | '\n' | 'u' | 'U' | 'N') {
            let mut diagnostic = Diagnostic::new(EscapeSequenceInDocstring, docstring.range());

            if !docstring.leading_quote().contains(['u', 'U']) {
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    "r".to_owned() + docstring.contents,
                    docstring.range(),
                )));
            }

            checker.diagnostics.push(diagnostic);
            break;
        }
    }
}
