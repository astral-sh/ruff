use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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

impl AlwaysAutofixableViolation for UnicodeKindPrefix {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Remove unicode literals from strings")
    }

    fn autofix_title(&self) -> String {
        "Remove unicode prefix".to_string()
    }
}

/// UP025
pub(crate) fn unicode_kind_prefix(checker: &mut Checker, expr: &Expr, kind: Option<&str>) {
    if matches!(kind, Some("u" | "U")) {
        let mut diagnostic = Diagnostic::new(UnicodeKindPrefix, expr.range());
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.set_fix(Fix::automatic(Edit::range_deletion(TextRange::at(
                expr.start(),
                TextSize::from(1),
            ))));
        }
        checker.diagnostics.push(diagnostic);
    }
}
