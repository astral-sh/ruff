use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ExprAttribute, ExprName, Identifier, Parameter};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for identifiers (variable names, function names, parameters, and
/// attribute accesses) that contain non-ASCII characters.
///
/// ## Why is this bad?
/// While Python (PEP 3131) allows non-ASCII characters in identifiers, they can
/// cause subtle bugs, reduce code readability, and make collaboration harder —
/// especially in teams where not everyone uses the same keyboard layout or locale.
///
/// Non-ASCII identifiers can also hide malicious code, such as confusable
/// homoglyph attacks where a Cyrillic "а" replaces a Latin "a".
///
/// Unlike [RUF001](ambiguous-unicode-character-string), which only catches
/// _confusable_ characters in strings, this rule flags _any_ non-ASCII
/// character in an identifier name.
///
/// ## Example
/// ```python
/// переменная = 42  # Cyrillic variable name
///
/// print(объект.метод)  # Cyrillic attribute access
/// ```
///
/// Use instead:
/// ```python
/// variable = 42
///
/// print(obj.method)
/// ```
///
/// ## References
/// - [PEP 3131 – Supporting Non-ASCII Identifiers](https://peps.python.org/pep-3131/)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.18")]
pub(crate) struct NonAsciiIdentifier {
    name: String,
}

impl Violation for NonAsciiIdentifier {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Identifier `{}` contains non-ASCII characters", self.name)
    }
}

/// RUF077
pub(crate) fn non_ascii_identifier_name(checker: &Checker, name: &ExprName) {
    if name.id.is_ascii() {
        return;
    }
    checker.report_diagnostic(
        NonAsciiIdentifier {
            name: name.id.to_string(),
        },
        name.range(),
    );
}

/// RUF077
pub(crate) fn non_ascii_identifier_attribute(checker: &Checker, attribute: &ExprAttribute) {
    if attribute.attr.is_ascii() {
        return;
    }
    checker.report_diagnostic(
        NonAsciiIdentifier {
            name: attribute.attr.to_string(),
        },
        attribute.range(),
    );
}

/// RUF077 — checks a function or class name (Identifier).
fn check_identifier(checker: &Checker, identifier: &Identifier) {
    if identifier.id.is_ascii() {
        return;
    }
    checker.report_diagnostic(
        NonAsciiIdentifier {
            name: identifier.id.to_string(),
        },
        identifier.range(),
    );
}

/// RUF077 — checks a function definition name.
pub(crate) fn non_ascii_identifier_function_def(checker: &Checker, name: &Identifier) {
    check_identifier(checker, name);
}

/// RUF077 — checks a class definition name.
pub(crate) fn non_ascii_identifier_class_def(checker: &Checker, name: &Identifier) {
    check_identifier(checker, name);
}

/// RUF077 — checks a parameter name.
pub(crate) fn non_ascii_identifier_parameter(checker: &Checker, parameter: &Parameter) {
    if parameter.name.id.is_ascii() {
        return;
    }
    checker.report_diagnostic(
        NonAsciiIdentifier {
            name: parameter.name.id.to_string(),
        },
        parameter.name.range(),
    );
}
