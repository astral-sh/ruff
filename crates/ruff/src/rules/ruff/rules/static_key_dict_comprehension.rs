use ruff_python_ast::Expr;

use crate::autofix::snippet::SourceCodeSnippet;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_constant;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for dictionary comprehensions that use a static key, like a string
/// literal.
///
/// ## Why is this bad?
/// Using a static key (like a string literal) in a dictionary comprehension
/// is usually a mistake, as it will result in a dictionary with only one key,
/// despite the comprehension iterating over multiple values.
///
/// ## Example
/// ```python
/// data = ["some", "Data"]
/// {"key": value.upper() for value in data}
/// ```
///
/// Use instead:
/// ```python
/// data = ["some", "Data"]
/// {value: value.upper() for value in data}
/// ```
#[violation]
pub struct StaticKeyDictComprehension {
    key: SourceCodeSnippet,
}

impl Violation for StaticKeyDictComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StaticKeyDictComprehension { key } = self;
        if let Some(key) = key.full_display() {
            format!("Dictionary comprehension uses static key: `{key}`")
        } else {
            format!("Dictionary comprehension uses static key")
        }
    }
}

/// RUF011
pub(crate) fn static_key_dict_comprehension(checker: &mut Checker, key: &Expr) {
    if is_constant(key) {
        checker.diagnostics.push(Diagnostic::new(
            StaticKeyDictComprehension {
                key: SourceCodeSnippet::from_str(checker.locator().slice(key)),
            },
            key.range(),
        ));
    }
}
