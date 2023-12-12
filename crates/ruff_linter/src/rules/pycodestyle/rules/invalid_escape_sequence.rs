use memchr::memchr_iter;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_python_parser::{StringKind, Tok};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::fix::edits::pad_start;

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
/// Or, if the string already contains a valid escape sequence:
/// ```python
/// value = "new line\nand invalid escape \_ here"
/// ```
///
/// Use instead:
/// ```python
/// value = "new line\nand invalid escape \\_ here"
/// ```
///
/// ## References
/// - [Python documentation: String and Bytes literals](https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals)
#[violation]
pub struct InvalidEscapeSequence {
    ch: char,
    fix_title: FixTitle,
}

impl AlwaysFixableViolation for InvalidEscapeSequence {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidEscapeSequence { ch, .. } = self;
        format!("Invalid escape sequence: `\\{ch}`")
    }

    fn fix_title(&self) -> String {
        match self.fix_title {
            FixTitle::AddBackslash => format!("Add backslash to escape sequence"),
            FixTitle::UseRawStringLiteral => format!("Use a raw string literal"),
        }
    }
}

/// W605
pub(crate) fn invalid_escape_sequence(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    indexer: &Indexer,
    token: &Tok,
    token_range: TextRange,
) {
    let (token_source_code, string_start_location) = match token {
        Tok::FStringMiddle { value, is_raw } => {
            if *is_raw {
                return;
            }
            let Some(range) = indexer.fstring_ranges().innermost(token_range.start()) else {
                return;
            };
            (value.as_str(), range.start())
        }
        Tok::String { kind, .. } => {
            if kind.is_raw() {
                return;
            }
            (locator.slice(token_range), token_range.start())
        }
        _ => return,
    };

    let mut contains_valid_escape_sequence = false;
    let mut invalid_escape_chars = Vec::new();

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
        invalid_escape_chars.push(InvalidEscapeChar {
            ch: next_char,
            range,
        });
    }

    let mut invalid_escape_sequence = Vec::new();
    if contains_valid_escape_sequence {
        // Escape with backslash.
        for invalid_escape_char in &invalid_escape_chars {
            let mut diagnostic = Diagnostic::new(
                InvalidEscapeSequence {
                    ch: invalid_escape_char.ch,
                    fix_title: FixTitle::AddBackslash,
                },
                invalid_escape_char.range(),
            );
            diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                r"\".to_string(),
                invalid_escape_char.start() + TextSize::from(1),
            )));
            invalid_escape_sequence.push(diagnostic);
        }
    } else {
        // Turn into raw string.
        for invalid_escape_char in &invalid_escape_chars {
            let mut diagnostic = Diagnostic::new(
                InvalidEscapeSequence {
                    ch: invalid_escape_char.ch,
                    fix_title: FixTitle::UseRawStringLiteral,
                },
                invalid_escape_char.range(),
            );

            if matches!(
                token,
                Tok::String {
                    kind: StringKind::Unicode,
                    ..
                }
            ) {
                // Replace the Unicode prefix with `r`.
                diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                    "r".to_string(),
                    string_start_location,
                    string_start_location + TextSize::from(1),
                )));
            } else {
                // Insert the `r` prefix.
                diagnostic.set_fix(
                    // If necessary, add a space between any leading keyword (`return`, `yield`,
                    // `assert`, etc.) and the string. For example, `return"foo"` is valid, but
                    // `returnr"foo"` is not.
                    Fix::safe_edit(Edit::insertion(
                        pad_start("r".to_string(), string_start_location, locator),
                        string_start_location,
                    )),
                );
            }

            invalid_escape_sequence.push(diagnostic);
        }
    }

    diagnostics.extend(invalid_escape_sequence);
}

#[derive(Debug, PartialEq, Eq)]
enum FixTitle {
    AddBackslash,
    UseRawStringLiteral,
}

#[derive(Debug)]
struct InvalidEscapeChar {
    ch: char,
    range: TextRange,
}

impl Ranged for InvalidEscapeChar {
    fn range(&self) -> TextRange {
        self.range
    }
}
