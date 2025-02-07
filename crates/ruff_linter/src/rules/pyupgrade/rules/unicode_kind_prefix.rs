use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::StringLiteral;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;

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
#[derive(ViolationMetadata)]
pub(crate) struct UnicodeKindPrefix;

impl AlwaysFixableViolation for UnicodeKindPrefix {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Remove unicode literals from strings".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove unicode prefix".to_string()
    }
}

/// UP025
pub(crate) fn unicode_kind_prefix(checker: &Checker, string: &StringLiteral) {
    if string.flags.prefix().is_unicode() {
        let mut diagnostic = Diagnostic::new(UnicodeKindPrefix, string.range);
        diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(TextRange::at(
            string.start(),
            TextSize::from(1),
        ))));
        checker.report_diagnostic(diagnostic);
    }
}
