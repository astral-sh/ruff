use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::Edit;
use ruff_diagnostics::{Diagnostic, DiagnosticKind, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use ruff_source_file::Locator;

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
#[violation]
pub struct InvalidCharacterBackspace;

impl AlwaysFixableViolation for InvalidCharacterBackspace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid unescaped character backspace, use \"\\b\" instead")
    }

    fn fix_title(&self) -> String {
        "Replace with escape sequence".to_string()
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
#[violation]
pub struct InvalidCharacterSub;

impl AlwaysFixableViolation for InvalidCharacterSub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid unescaped character SUB, use \"\\x1A\" instead")
    }

    fn fix_title(&self) -> String {
        "Replace with escape sequence".to_string()
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
#[violation]
pub struct InvalidCharacterEsc;

impl AlwaysFixableViolation for InvalidCharacterEsc {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid unescaped character ESC, use \"\\x1B\" instead")
    }

    fn fix_title(&self) -> String {
        "Replace with escape sequence".to_string()
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
#[violation]
pub struct InvalidCharacterNul;

impl AlwaysFixableViolation for InvalidCharacterNul {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid unescaped character NUL, use \"\\0\" instead")
    }

    fn fix_title(&self) -> String {
        "Replace with escape sequence".to_string()
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
#[violation]
pub struct InvalidCharacterZeroWidthSpace;

impl AlwaysFixableViolation for InvalidCharacterZeroWidthSpace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid unescaped character zero-width-space, use \"\\u200B\" instead")
    }

    fn fix_title(&self) -> String {
        "Replace with escape sequence".to_string()
    }
}

/// PLE2510, PLE2512, PLE2513, PLE2514, PLE2515
pub(crate) fn invalid_string_characters(
    diagnostics: &mut Vec<Diagnostic>,
    token: TokenKind,
    range: TextRange,
    locator: &Locator,
) {
    let text = match token {
        // We can't use the `value` field since it's decoded and e.g. for f-strings removed a curly
        // brace that escaped another curly brace, which would gives us wrong column information.
        TokenKind::String | TokenKind::FStringMiddle => locator.slice(range),
        _ => return,
    };

    for (column, match_) in text.match_indices(&['\x08', '\x1A', '\x1B', '\0', '\u{200b}']) {
        let c = match_.chars().next().unwrap();
        let (replacement, rule): (&str, DiagnosticKind) = match c {
            '\x08' => ("\\b", InvalidCharacterBackspace.into()),
            '\x1A' => ("\\x1A", InvalidCharacterSub.into()),
            '\x1B' => ("\\x1B", InvalidCharacterEsc.into()),
            '\0' => ("\\0", InvalidCharacterNul.into()),
            '\u{200b}' => ("\\u200b", InvalidCharacterZeroWidthSpace.into()),
            _ => {
                continue;
            }
        };

        let location = range.start() + TextSize::try_from(column).unwrap();
        let range = TextRange::at(location, c.text_len());

        diagnostics.push(Diagnostic::new(rule, range).with_fix(Fix::safe_edit(
            Edit::range_replacement(replacement.to_string(), range),
        )));
    }
}
