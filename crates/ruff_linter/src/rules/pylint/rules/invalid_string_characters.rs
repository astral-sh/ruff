use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::{Token, TokenKind};
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
#[violation_metadata(stable_since = "v0.0.257")]
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
/// By using the `\x1a` sequence in lieu of the `SUB` control character, the
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
#[violation_metadata(stable_since = "v0.0.257")]
pub(crate) struct InvalidCharacterSub;

impl Violation for InvalidCharacterSub {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid unescaped character SUB, use \"\\x1a\" instead".to_string()
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
/// By using the `\x1b` sequence in lieu of the `ESC` control character, the
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
#[violation_metadata(stable_since = "v0.0.257")]
pub(crate) struct InvalidCharacterEsc;

impl Violation for InvalidCharacterEsc {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid unescaped character ESC, use \"\\x1b\" instead".to_string()
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
#[violation_metadata(stable_since = "v0.0.257")]
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
#[violation_metadata(stable_since = "v0.0.257")]
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
        TokenKind::String | TokenKind::FStringMiddle | TokenKind::TStringMiddle => {
            locator.slice(token)
        }
        _ => return,
    };

    for (column, match_) in text.match_indices(&['\x08', '\x1a', '\x1b', '\0', '\u{200b}']) {
        let location = token.start() + TextSize::try_from(column).unwrap();
        let c = match_.chars().next().unwrap();
        let range = TextRange::at(location, c.text_len());

        let is_escaped = &text[..column]
            .chars()
            .rev()
            .take_while(|c| *c == '\\')
            .count()
            % 2
            == 1;

        let (replacement, diagnostic) = match c {
            '\x08' => (
                "\\b",
                context.report_diagnostic_if_enabled(InvalidCharacterBackspace, range),
            ),
            '\x1a' => (
                "\\x1a",
                context.report_diagnostic_if_enabled(InvalidCharacterSub, range),
            ),
            '\x1b' => (
                "\\x1b",
                context.report_diagnostic_if_enabled(InvalidCharacterEsc, range),
            ),
            '\0' => (
                "\\0",
                context.report_diagnostic_if_enabled(InvalidCharacterNul, range),
            ),
            '\u{200b}' => (
                "\\u200b",
                context.report_diagnostic_if_enabled(InvalidCharacterZeroWidthSpace, range),
            ),
            _ => {
                continue;
            }
        };

        let Some(mut diagnostic) = diagnostic else {
            continue;
        };

        if !token.unwrap_string_flags().is_raw_string() && !is_escaped {
            let edit = Edit::range_replacement(replacement.to_string(), range);
            diagnostic.set_fix(Fix::safe_edit(edit));
        }
    }
}
