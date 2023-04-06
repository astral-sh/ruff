use rustpython_parser::ast::Location;

use ruff_diagnostics::AlwaysAutofixableViolation;
use ruff_diagnostics::Edit;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;
use ruff_python_ast::newlines::UniversalNewlineIterator;
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

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
/// x = ''
/// ```
///
/// Use instead:
/// ```python
/// x = '\b'
/// ```
#[violation]
pub struct InvalidCharacterBackspace;

impl AlwaysAutofixableViolation for InvalidCharacterBackspace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid unescaped character backspace, use \"\\b\" instead")
    }

    fn autofix_title(&self) -> String {
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
/// x = ''
/// ```
///
/// Use instead:
/// ```python
/// x = '\x1A'
/// ```
#[violation]
pub struct InvalidCharacterSub;

impl AlwaysAutofixableViolation for InvalidCharacterSub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid unescaped character SUB, use \"\\x1A\" instead")
    }

    fn autofix_title(&self) -> String {
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
/// x = ''
/// ```
///
/// Use instead:
/// ```python
/// x = '\x1B'
/// ```
#[violation]
pub struct InvalidCharacterEsc;

impl AlwaysAutofixableViolation for InvalidCharacterEsc {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid unescaped character ESC, use \"\\x1B\" instead")
    }

    fn autofix_title(&self) -> String {
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
/// x = ''
/// ```
///
/// Use instead:
/// ```python
/// x = '\0'
/// ```
#[violation]
pub struct InvalidCharacterNul;

impl AlwaysAutofixableViolation for InvalidCharacterNul {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid unescaped character NUL, use \"\\0\" instead")
    }

    fn autofix_title(&self) -> String {
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
/// x = 'Dear Sir/Madam'
/// ```
///
/// Use instead:
/// ```python
/// x = 'Dear Sir\u200B/\u200BMadam'  # zero width space
/// ```
#[violation]
pub struct InvalidCharacterZeroWidthSpace;

impl AlwaysAutofixableViolation for InvalidCharacterZeroWidthSpace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid unescaped character zero-width-space, use \"\\u200B\" instead")
    }

    fn autofix_title(&self) -> String {
        "Replace with escape sequence".to_string()
    }
}

/// PLE2510, PLE2512, PLE2513, PLE2514, PLE2515
pub fn invalid_string_characters(
    locator: &Locator,
    start: Location,
    end: Location,
    autofix: bool,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let text = locator.slice(Range::new(start, end));

    for (row, line) in UniversalNewlineIterator::from(text).enumerate() {
        let mut char_offset = 0;
        for char in line.chars() {
            let (replacement, rule): (&str, DiagnosticKind) = match char {
                '\x08' => ("\\b", InvalidCharacterBackspace.into()),
                '\x1A' => ("\\x1A", InvalidCharacterSub.into()),
                '\x1B' => ("\\x1B", InvalidCharacterEsc.into()),
                '\0' => ("\\0", InvalidCharacterNul.into()),
                '\u{200b}' => ("\\u200b", InvalidCharacterZeroWidthSpace.into()),
                _ => {
                    char_offset += 1;
                    continue;
                }
            };
            let location = helpers::to_absolute(Location::new(row + 1, char_offset), start);
            let end_location = Location::new(location.row(), location.column() + 1);
            let mut diagnostic = Diagnostic::new(rule, Range::new(location, end_location));
            if autofix {
                diagnostic.set_fix(Edit::replacement(
                    replacement.to_string(),
                    location,
                    end_location,
                ));
            }
            diagnostics.push(diagnostic);
            char_offset += 1;
        }
    }

    diagnostics
}
