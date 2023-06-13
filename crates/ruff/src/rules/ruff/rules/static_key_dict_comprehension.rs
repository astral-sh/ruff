use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_constant;

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
    key: String,
}

impl Violation for StaticKeyDictComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StaticKeyDictComprehension { key } = self;
        format!("Dictionary comprehension uses static key: `{key}`")
    }
}

/// RUF011
pub(crate) fn static_key_dict_comprehension(checker: &mut Checker, key: &Expr) {
    if is_constant(key) {
        checker.diagnostics.push(Diagnostic::new(
            StaticKeyDictComprehension {
                key: checker.locator.slice(key.range()).to_string(),
            },
            key.range(),
        ));
    }
}
