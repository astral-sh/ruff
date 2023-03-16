use rustpython_parser::ast::Location;

use ruff_macros::{derive_message_formats, violation};

use ruff_diagnostics::AlwaysAutofixableViolation;
use ruff_diagnostics::Fix;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

/// ## What it does
/// Checks that strings don't have the control character BS
///
/// ## Why is this bad?
/// Control characters can display differently in different text editors and terminals. By using the \b sequence the string's
/// value will be the same but it will be visible in all text editors.
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
/// Checks that strings don't have the raw control character SUB
///
/// ## Why is this bad?
/// Control characters can display differently in different text editors and terminals. By using the \x1B sequence the string's
/// value will be the same but it will be visible in all text editors.
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
/// Checks that strings don't have the raw control character ESC
///
/// ## Why is this bad?
/// Control characters can display differently in different text editors and terminals. By using the \x1B sequence the string's
/// value will be the same but it will be visible in all text editors.
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
/// Checks that strings don't have the raw control character NUL (0 byte)
///
/// ## Why is this bad?
/// Control characters can display differently in different text editors and terminals. By using the \x1B sequence the string's
/// value will be the same but it will be visible in all text editors.
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
/// Checks that strings don't have the zero width space character
///
/// ## Why is this bad?
/// This character can be invisible in some text editors and terminals. By using the \x1B sequence the string's
/// value will be the same but it will be visible in all text editors.
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

    for (row_offset, line) in UniversalNewlineIterator::from(text).enumerate() {
        for (col_offset, m) in line.match_indices(&['\x08', '\x1A', '\x1B', '\0', '\u{200b}']) {
            let col = if row_offset == 0 {
                start.column() + col_offset
            } else {
                col_offset
            };
            let (replacement, rule): (&str, DiagnosticKind) = match m.chars().next().unwrap() {
                '\x08' => ("\\b", InvalidCharacterBackspace.into()),
                '\x1A' => ("\\x1A", InvalidCharacterSub.into()),
                '\x1B' => ("\\x1B", InvalidCharacterEsc.into()),
                '\0' => ("\\0", InvalidCharacterNul.into()),
                '\u{200b}' => ("\\u200b", InvalidCharacterZeroWidthSpace.into()),
                _ => unreachable!(),
            };
            let location = Location::new(start.row() + row_offset, col);
            let end_location = Location::new(location.row(), location.column() + 1);
            let mut diagnostic = Diagnostic::new(rule, Range::new(location, end_location));
            if autofix {
                diagnostic.amend(Fix::replacement(
                    replacement.to_string(),
                    location,
                    end_location,
                ));
            }
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}
