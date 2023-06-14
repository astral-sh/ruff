use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for unicode literals in strings.
///
/// ## Why is this bad?
/// In Python 3, all strings are unicode by default. Unicode literals should be
/// replaced with regular strings to avoid confusion.
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
    if let Some(const_kind) = kind {
        if const_kind.to_lowercase() == "u" {
            let mut diagnostic = Diagnostic::new(UnicodeKindPrefix, expr.range());
            if checker.patch(diagnostic.kind.rule()) {
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::range_deletion(TextRange::at(
                    expr.start(),
                    TextSize::from(1),
                ))));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
