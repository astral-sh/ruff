use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_parser::{Token, TokenKind};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::Locator;
use crate::checkers::ast::LintContext;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for strings that contain the control character `BS`.
///
/// ## Why is this bad?
/// Control characters are displayed differently by different text editors and
/// terminals.
///
/// By using the `\b` sequence in lieu of the `BS` control character, the
/// string will contain the same value, but will render visibly in all editors.
///
/// ## Example
/// ```python
/// x = ""
/// ```
///
/// Use instead:
/// ```python
/// x = "\b"
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct InvalidCharacterBackspace;

impl Violation for InvalidCharacterBackspace {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid unescaped character backspace, use \"\\b\" instead".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with escape sequence".to_string())
    }
}

/// ## What it does
/// Checks for strings that contain the raw control character `SUB`.
///
/// ## Why is this bad?
/// Control characters are displayed differently by different text editors and
/// terminals.
///
/// By using the `\x1A` sequence in lieu of the `SUB` control character, the
/// string will contain the same value, but will render visibly in all editors.
///
/// ## Example
/// ```python
/// x = ""
/// ```
///
/// Use instead:
/// ```python
/// x = "\x1a"
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct InvalidCharacterSub;

impl Violation for InvalidCharacterSub {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid unescaped character SUB, use \"\\x1A\" instead".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with escape sequence".to_string())
    }
}

/// ## What it does
/// Checks for strings that contain the raw control character `ESC`.
///
/// ## Why is this bad?
/// Control characters are displayed differently by different text editors and
/// terminals.
///
/// By using the `\x1B` sequence in lieu of the `SUB` control character, the
/// string will contain the same value, but will render visibly in all editors.
///
/// ## Example
/// ```python
/// x = ""
/// ```
///
/// Use instead:
/// ```python
/// x = "\x1b"
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct InvalidCharacterEsc;

impl Violation for InvalidCharacterEsc {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid unescaped character ESC, use \"\\x1B\" instead".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with escape sequence".to_string())
    }
}

/// ## What it does
/// Checks for strings that contain the raw control character `NUL` (0 byte).
///
/// ## Why is this bad?
/// Control characters are displayed differently by different text editors and
/// terminals.
///
/// By using the `\0` sequence in lieu of the `NUL` control character, the
/// string will contain the same value, but will render visibly in all editors.
///
/// ## Example
/// ```python
/// x = ""
/// ```
///
/// Use instead:
/// ```python
/// x = "\0"
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct InvalidCharacterNul;

impl Violation for InvalidCharacterNul {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid unescaped character NUL, use \"\\0\" instead".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with escape sequence".to_string())
    }
}

/// ## What it does
/// Checks for strings that contain the zero width space character.
///
/// ## Why is this bad?
/// This character is rendered invisibly in some text editors and terminals.
///
/// By using the `\u200B` sequence, the string will contain the same value,
/// but will render visibly in all editors.
///
/// ## Example
/// ```python
/// x = "Dear Sir/Madam"
/// ```
///
/// Use instead:
/// ```python
/// x = "Dear Sir\u200b/\u200bMadam"  # zero width space
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct InvalidCharacterZeroWidthSpace;

impl Violation for InvalidCharacterZeroWidthSpace {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid unescaped character zero-width-space, use \"\\u200B\" instead".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with escape sequence".to_string())
    }
}

/// PLE2510, PLE2512, PLE2513, PLE2514, PLE2515
pub(crate) fn invalid_string_characters(context: &LintContext, token: &Token, locator: &Locator) {
    let text = match token.kind() {
        // We can't use the `value` field since it's decoded and e.g. for f-strings removed a curly
        // brace that escaped another curly brace, which would gives us wrong column information.
        TokenKind::String | TokenKind::FStringMiddle => locator.slice(token),
        _ => return,
    };

    for (column, match_) in text.match_indices(&['\x08', '\x1A', '\x1B', '\0', '\u{200b}']) {
        let location = token.start() + TextSize::try_from(column).unwrap();
        let c = match_.chars().next().unwrap();
        let range = TextRange::at(location, c.text_len());
        // To correctly check for preceding backslashes, we need the character's
        // position relative to the start of the string's content, not the start of the token.
        //
        // For regular string tokens, the content starts after the opening quote, so we offset the column by -1.
        // F-string middle tokens don't have surrounding quotes, so the column is the correct position.
        let pos = if token.kind() == TokenKind::FStringMiddle {
            column
        } else {
            column - 1
        };
        let (replacement, diagnostic, is_fix_available) = match c {
            '\x08' => (
                "\\b",
                context.report_diagnostic_if_enabled(InvalidCharacterBackspace, range),
                is_preceded_by_even_backslashes(text, pos),
            ),
            '\x1A' => (
                "\\x1A",
                context.report_diagnostic_if_enabled(InvalidCharacterSub, range),
                is_preceded_by_even_backslashes(text, pos),
            ),
            '\x1B' => (
                "\\x1B",
                context.report_diagnostic_if_enabled(InvalidCharacterEsc, range),
                is_preceded_by_even_backslashes(text, pos),
            ),
            '\0' => (
                "\\0",
                context.report_diagnostic_if_enabled(InvalidCharacterNul, range),
                is_preceded_by_even_backslashes(text, pos),
            ),
            '\u{200b}' => (
                "\\u200b",
                context.report_diagnostic_if_enabled(InvalidCharacterZeroWidthSpace, range),
                is_preceded_by_even_backslashes(text, pos),
            ),
            _ => {
                continue;
            }
        };

        let Some(mut diagnostic) = diagnostic else {
            continue;
        };

        if !token.unwrap_string_flags().is_raw_string() && is_fix_available {
            let edit = Edit::range_replacement(replacement.to_string(), range);
            diagnostic.set_fix(Fix::safe_edit(edit));
        }
    }
}

/// Check if `text` is preceded by an even number of backslashes.
/// `pos` is the position where we should start looking for backslashes in `text`.
fn is_preceded_by_even_backslashes(text: &str, pos: usize) -> bool {
    let backslash_count = text.chars().skip(pos).take_while(|c| *c == '\\').count();
    backslash_count % 2 == 0
}
