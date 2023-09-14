use memchr::memchr_iter;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_python_parser::Tok;
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
    indexer: &Indexer,
    token: &Tok,
    token_range: TextRange,
    autofix: bool,
) {
    let token_source_code = match token {
        Tok::FStringMiddle { value, is_raw } => {
            if *is_raw {
                return;
            }
            value.as_str()
        }
        Tok::String { kind, .. } => {
            if kind.is_raw() {
                return;
            }
            locator.slice(token_range)
        }
        _ => return,
    };

    let mut contains_valid_escape_sequence = false;
    let mut invalid_escape_sequence = Vec::new();

    let mut prev = None;
    let bytes = token_source_code.as_bytes();
    for i in memchr_iter(b'\\', bytes) {
        // If the previous character was also a backslash, skip.
        if prev.is_some_and(|prev| prev == i - 1) {
            prev = None;
            continue;
        }

        prev = Some(i);

        let next_char = match token_source_code[i + 1..].chars().next() {
            Some(next_char) => next_char,
            None if token.is_f_string_middle() => {
                // If we're at the end of a f-string middle token, the next character
                // is actually emitted as a different token. For example,
                //
                // ```python
                // f"\{1}"
                // ```
                //
                // is lexed as `FStringMiddle('\\')` and `LBrace` (ignoring irrelevant
                // tokens), so we need to check the next character in the source code.
                //
                // Now, if we're at the end of the f-string itself, the lexer wouldn't
                // have emitted the `FStringMiddle` token in the first place. For example,
                //
                // ```python
                // f"foo\"
                // ```
                //
                // Here, there won't be any `FStringMiddle` because it's an unterminated
                // f-string. This means that if there's a `FStringMiddle` token and we
                // encounter a `\` character, then the next character is always going to
                // be part of the f-string.
                if let Some(next_char) = locator.after(token_range.end()).chars().next() {
                    next_char
                } else {
                    continue;
                }
            }
            // If we're at the end of the file, skip.
            None => continue,
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

        let location = token_range.start() + TextSize::try_from(i).unwrap();
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
            let tok_start = if token.is_f_string_middle() {
                // SAFETY: If this is a `FStringMiddle` token, then the indexer
                // must have the f-string range.
                indexer
                    .fstring_ranges()
                    .innermost(token_range.start())
                    .unwrap()
                    .start()
            } else {
                token_range.start()
            };
            // Turn into raw string.
            for diagnostic in &mut invalid_escape_sequence {
                // If necessary, add a space between any leading keyword (`return`, `yield`,
                // `assert`, etc.) and the string. For example, `return"foo"` is valid, but
                // `returnr"foo"` is not.
                diagnostic.set_fix(Fix::automatic(Edit::insertion(
                    pad_start("r".to_string(), tok_start, locator),
                    tok_start,
                )));
            }
        }
    }

    diagnostics.extend(invalid_escape_sequence);
}
