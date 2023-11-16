use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::{StringKind, Tok};

use ruff_text_size::{Ranged, TextRange, TextSize};

/// ## What it does
/// Checks for uses of the Unicode kind prefix (`u`) in strings.
///
/// ## Why is this bad?
/// In Python 3, all strings are Unicode by default. The Unicode kind prefix is
/// unnecessary and should be removed to avoid confusion.
///
/// ## Example
/// ```python
/// u"foo"
/// ```
///
/// Use instead:
/// ```python
/// "foo"
/// ```
///
/// ## References
/// - [Python documentation: Unicode HOWTO](https://docs.python.org/3/howto/unicode.html)
#[violation]
pub struct UnicodeKindPrefix;

impl AlwaysFixableViolation for UnicodeKindPrefix {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Remove unicode literals from strings")
    }

    fn fix_title(&self) -> String {
        "Remove unicode prefix".to_string()
    }
}

/// UP025
pub(crate) fn unicode_kind_prefix(diagnostics: &mut Vec<Diagnostic>, tokens: &[LexResult]) {
    for (token, range) in tokens.iter().flatten() {
        if let Tok::String {
            kind: StringKind::Unicode,
            ..
        } = token
        {
            let mut diagnostic = Diagnostic::new(UnicodeKindPrefix, *range);
            diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(TextRange::at(
                range.start(),
                TextSize::from(1),
            ))));
            diagnostics.push(diagnostic);
        }
    }
}
