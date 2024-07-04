use memchr::memchr_iter;

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
    let mut backslash_index = 0;
    let escaped_docstring_backslashes_pattern = b"\"\\\"\\\"";
    if memchr_iter(b'\\', bytes).any(|position| {
        let escaped_char = bytes.get(position.saturating_add(1));
        // Allow escaped docstring.
        if matches!(escaped_char, Some(b'"')) {
            // If the next chars is equal to `"""`, it is a escaped docstring pattern.
            let escaped_triple_quotes =
                &bytes[position.saturating_add(1)..position.saturating_add(4)];
            if escaped_triple_quotes == b"\"\"\"" {
                return false;
            }
            // For the `"\"\"` pattern, each iteration advances by 2 characters.
            // For example, the sequence progresses from `"\"\"` to `"\"` and then to `"`.
            // Therefore, we utilize an index to keep track of the remaining characters.
            let escaped_quotes_backslashes = &bytes
                [position.saturating_add(1)..position.saturating_add(6 - backslash_index * 2)];
            if escaped_quotes_backslashes
                == &escaped_docstring_backslashes_pattern[backslash_index * 2..]
            {
                backslash_index += 1;
                // Reset to avoid overflow.
                if backslash_index > 2 {
                    backslash_index = 0;
                }
                return false;
            }
            return true;
        }
        // Allow continuations (backslashes followed by newlines) and Unicode escapes.
        !matches!(escaped_char, Some(b'\r' | b'\n' | b'u' | b'U' | b'N'))
    }) {
        let mut diagnostic = Diagnostic::new(EscapeSequenceInDocstring, docstring.range());

        if !docstring.leading_quote().contains(['u', 'U']) {
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                "r".to_owned() + docstring.contents,
                docstring.range(),
            )));
        }

        checker.diagnostics.push(diagnostic);
    }
}
