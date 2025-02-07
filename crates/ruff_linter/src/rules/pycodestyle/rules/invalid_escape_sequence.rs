use memchr::memchr_iter;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{AnyStringFlags, FStringElement, StringLike, StringLikePart};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::fix::edits::pad_start;
use crate::Locator;

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
#[derive(ViolationMetadata)]
pub(crate) struct InvalidEscapeSequence {
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
            FixTitle::AddBackslash => "Add backslash to escape sequence".to_string(),
            FixTitle::UseRawStringLiteral => "Use a raw string literal".to_string(),
        }
    }
}

/// W605
pub(crate) fn invalid_escape_sequence(checker: &Checker, string_like: StringLike) {
    let locator = checker.locator();

    for part in string_like.parts() {
        if part.flags().is_raw_string() {
            continue;
        }
        let state = match part {
            StringLikePart::String(_) | StringLikePart::Bytes(_) => {
                analyze_escape_chars(locator, part.range(), part.flags())
            }
            StringLikePart::FString(f_string) => {
                let flags = AnyStringFlags::from(f_string.flags);
                let mut escape_chars_state = EscapeCharsState::default();
                // Whether we suggest converting to a raw string or
                // adding backslashes depends on the presence of valid
                // escape characters in the entire f-string. Therefore,
                // we must analyze escape characters in each f-string
                // element before pushing a diagnostic and fix.
                for element in &f_string.elements {
                    match element {
                        FStringElement::Literal(literal) => {
                            escape_chars_state.update(analyze_escape_chars(
                                locator,
                                literal.range(),
                                flags,
                            ));
                        }
                        FStringElement::Expression(expression) => {
                            let Some(format_spec) = expression.format_spec.as_ref() else {
                                continue;
                            };
                            for literal in format_spec.elements.literals() {
                                escape_chars_state.update(analyze_escape_chars(
                                    locator,
                                    literal.range(),
                                    flags,
                                ));
                            }
                        }
                    }
                }
                escape_chars_state
            }
        };
        check(checker, locator, part.start(), part.flags(), state);
    }
}

#[derive(Default)]
struct EscapeCharsState {
    contains_valid_escape_sequence: bool,
    invalid_escape_chars: Vec<InvalidEscapeChar>,
}

impl EscapeCharsState {
    fn update(&mut self, other: Self) {
        self.contains_valid_escape_sequence |= other.contains_valid_escape_sequence;
        self.invalid_escape_chars.extend(other.invalid_escape_chars);
    }
}

/// Traverses string, collects invalid escape characters, and flags if a valid
/// escape character is found.
fn analyze_escape_chars(
    locator: &Locator,
    // Range in the source code to perform the analysis on.
    source_range: TextRange,
    flags: AnyStringFlags,
) -> EscapeCharsState {
    let source = locator.slice(source_range);
    let mut contains_valid_escape_sequence = false;
    let mut invalid_escape_chars = Vec::new();

    let mut prev = None;
    let bytes = source.as_bytes();
    for i in memchr_iter(b'\\', bytes) {
        // If the previous character was also a backslash, skip.
        if prev.is_some_and(|prev| prev == i - 1) {
            prev = None;
            continue;
        }

        prev = Some(i);

        let next_char = match source[i + 1..].chars().next() {
            Some(next_char) => next_char,
            None if flags.is_f_string() => {
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
                if let Some(next_char) = locator.after(source_range.end()).chars().next() {
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
            contains_valid_escape_sequence = true;
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

        let location = source_range.start() + TextSize::try_from(i).unwrap();
        let range = TextRange::at(location, next_char.text_len() + TextSize::from(1));
        invalid_escape_chars.push(InvalidEscapeChar {
            ch: next_char,
            range,
        });
    }
    EscapeCharsState {
        contains_valid_escape_sequence,
        invalid_escape_chars,
    }
}

/// Pushes a diagnostic and fix depending on escape characters seen so far.
///
/// If we have not seen any valid escape characters, we convert to
/// a raw string. If we have seen valid escape characters,
/// we manually add backslashes to each invalid escape character found.
fn check(
    checker: &Checker,
    locator: &Locator,
    // Start position of the expression that contains the source range. This is used to generate
    // the fix when the source range is part of the expression like in f-string which contains
    // other f-string literal elements.
    expr_start: TextSize,
    flags: AnyStringFlags,
    escape_chars_state: EscapeCharsState,
) {
    let EscapeCharsState {
        contains_valid_escape_sequence,
        invalid_escape_chars,
    } = escape_chars_state;
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
            checker.report_diagnostic(diagnostic);
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

            if flags.is_u_string() {
                // Replace the Unicode prefix with `r`.
                diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                    "r".to_string(),
                    expr_start,
                    expr_start + TextSize::from(1),
                )));
            } else {
                // Insert the `r` prefix.
                diagnostic.set_fix(
                    // If necessary, add a space between any leading keyword (`return`, `yield`,
                    // `assert`, etc.) and the string. For example, `return"foo"` is valid, but
                    // `returnr"foo"` is not.
                    Fix::safe_edit(Edit::insertion(
                        pad_start("r".to_string(), expr_start, locator),
                        expr_start,
                    )),
                );
            }

            checker.report_diagnostic(diagnostic);
        }
    }
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
