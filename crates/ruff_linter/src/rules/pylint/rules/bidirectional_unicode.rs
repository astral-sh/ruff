use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_source_file::Line;

const BIDI_UNICODE: [char; 10] = [
    '\u{202A}', //{LEFT-TO-RIGHT EMBEDDING}
    '\u{202B}', //{RIGHT-TO-LEFT EMBEDDING}
    '\u{202C}', //{POP DIRECTIONAL FORMATTING}
    '\u{202D}', //{LEFT-TO-RIGHT OVERRIDE}
    '\u{202E}', //{RIGHT-TO-LEFT OVERRIDE}
    '\u{2066}', //{LEFT-TO-RIGHT ISOLATE}
    '\u{2067}', //{RIGHT-TO-LEFT ISOLATE}
    '\u{2068}', //{FIRST STRONG ISOLATE}
    '\u{2069}', //{POP DIRECTIONAL ISOLATE}
    // The following was part of PEP 672:
    // https://peps.python.org/pep-0672/
    // so the list above might not be complete
    '\u{200F}', //{RIGHT-TO-LEFT MARK}
                // We don't use
                //   "\u200E" # \n{LEFT-TO-RIGHT MARK}
                // as this is the default for latin files and can't be used
                // to hide code
];

/// ## What it does
/// Checks for bidirectional unicode characters.
///
/// ## Why is this bad?
/// The interaction between bidirectional unicode characters and the
/// surrounding code can be surprising to those that are unfamiliar
/// with right-to-left writing systems.
///
/// In some cases, bidirectional unicode characters can also be used to
/// obfuscate code and introduce or mask security vulnerabilities.
///
/// ## Example
/// ```python
/// s = "א" * 100  #  "א" is assigned
/// print(s)  # prints a 100-character string
/// ```
///
/// ## References
/// - [PEP 672: Bidirectional Text](https://peps.python.org/pep-0672/#bidirectional-text)
#[derive(ViolationMetadata)]
pub(crate) struct BidirectionalUnicode;

impl Violation for BidirectionalUnicode {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Contains control characters that can permit obfuscated code".to_string()
    }
}

/// PLE2502
pub(crate) fn bidirectional_unicode(line: &Line) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if line.contains(BIDI_UNICODE) {
        diagnostics.push(Diagnostic::new(BidirectionalUnicode, line.full_range()));
    }
    diagnostics
}
