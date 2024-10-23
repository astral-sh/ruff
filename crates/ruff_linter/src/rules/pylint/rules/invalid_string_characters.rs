use ruff_python_ast::str::Quote;
use ruff_python_ast::StringFlags;
use ruff_python_parser::Token;
use ruff_text_size::Ranged;
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
pub(crate) fn invalid_string_characters<'a>(
    diagnostics: &mut Vec<Diagnostic>,
    token: &'a Token,
    last_fstring_start: &mut Option<&'a Token>,
    locator: &Locator,
) {
    struct InvalidCharacterDiagnostic {
        diagnostic: Diagnostic,
        edit: Edit,
    }

    let kind = token.kind();
    let range = token.range();

    let text = match kind {
        // We can't use the `value` field since it's decoded and e.g. for f-strings removed a curly
        // brace that escaped another curly brace, which would gives us wrong column information.
        TokenKind::String | TokenKind::FStringMiddle => locator.slice(range),
        TokenKind::FStringStart => {
            *last_fstring_start = Some(token);
            return;
        }
        _ => return,
    };

    // Accumulate diagnostics here to postpone generating shared fixes until we know we need them.
    let mut new_diagnostics: Vec<InvalidCharacterDiagnostic> = Vec::new();
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

        new_diagnostics.push(InvalidCharacterDiagnostic {
            diagnostic: Diagnostic::new(rule, range),
            // This is integrated with other fixes and attached to the diagnostic below.
            edit: Edit::range_replacement(replacement.to_string(), range),
        });
    }
    if new_diagnostics.is_empty() {
        // No issues, nothing to fix.
        return;
    }

    // Convert raw strings to non-raw strings when fixes are applied:
    // https://github.com/astral-sh/ruff/issues/13294#issuecomment-2341955180
    let mut string_conversion_edits = Vec::new();
    if token.is_raw_string() {
        let string_flags = token.string_flags();
        let prefix = string_flags.prefix().as_str();

        // 1. Remove the raw string prefix.
        for (column, match_) in prefix.match_indices(&['r', 'R']) {
            let c = match_.chars().next().unwrap();

            let entire_string_range = match kind {
                TokenKind::String => range,
                _ => last_fstring_start.unwrap().range(),
            };
            let location = entire_string_range.start() + TextSize::try_from(column).unwrap();
            let range = TextRange::at(location, c.text_len());

            string_conversion_edits.push(Edit::range_deletion(range));
        }

        // 2. Escape '\' and quote characters inside the string content.
        let (content_start, content_end): (TextSize, TextSize) = match kind {
            TokenKind::String => (
                prefix.text_len() + string_flags.quote_len(),
                TextSize::try_from(text.len()).unwrap() - string_flags.quote_len(),
            ),
            _ => (0.into(), text.len().try_into().unwrap()),
        };
        let string_content = &text[content_start.to_usize()..content_end.to_usize()];
        for (column, match_) in string_content.match_indices(&['\\', '\'', '"']) {
            let c = match_.chars().next().unwrap();
            let replacement: &str = match c {
                '\\' => "\\\\",
                '\'' | '"' => {
                    if string_flags.is_triple_quoted() {
                        continue;
                    }
                    match (c, string_flags.quote_style()) {
                        ('\'', Quote::Single) => "\\'",
                        ('"', Quote::Double) => "\\\"",
                        _ => {
                            continue;
                        }
                    }
                }
                _ => {
                    continue;
                }
            };

            let location = range.start() + content_start + TextSize::try_from(column).unwrap();
            let range = TextRange::at(location, c.text_len());

            string_conversion_edits.push(Edit::range_replacement(replacement.to_string(), range));
        }

        // 3. Add back '\' characters for line continuation in non-triple-quoted strings.
        if !string_flags.is_triple_quoted() {
            for (column, _match) in string_content.match_indices("\\\n") {
                let location = range.start() + content_start + TextSize::try_from(column).unwrap();
                string_conversion_edits.push(Edit::insertion(
                    "\\n\\".to_string(),
                    location + TextSize::from(1),
                ));
            }
        }
    }

    for InvalidCharacterDiagnostic { diagnostic, edit } in new_diagnostics {
        diagnostics
            .push(diagnostic.with_fix(Fix::safe_edits(edit, string_conversion_edits.clone())));
    }
}
