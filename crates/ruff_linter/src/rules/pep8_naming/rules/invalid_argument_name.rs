use ruff_python_ast::Parameter;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_stdlib::str;
use ruff_text_size::Ranged;

use crate::rules::pep8_naming::settings::IgnoreNames;

/// ## What it does
/// Checks for argument names that do not follow the `snake_case` convention.
///
/// ## Why is this bad?
/// [PEP 8] recommends that function names should be lower case and separated
/// by underscores (also known as `snake_case`).
///
/// > Function names should be lowercase, with words separated by underscores
/// as necessary to improve readability.
/// >
/// > Variable names follow the same convention as function names.
/// >
/// > mixedCase is allowed only in contexts where that’s already the
/// prevailing style (e.g. threading.py), to retain backwards compatibility.
///
/// ## Example
/// ```python
/// def my_function(A, myArg):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def my_function(a, my_arg):
///     pass
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-method-arguments
#[violation]
pub struct InvalidArgumentName {
    name: String,
}

impl Violation for InvalidArgumentName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidArgumentName { name } = self;
        format!("Argument name `{name}` should be lowercase")
    }
}

/// N803
pub(crate) fn invalid_argument_name(
    name: &str,
    parameter: &Parameter,
    ignore_names: &IgnoreNames,
) -> Option<Diagnostic> {
    if !str::is_lowercase(name) {
        // Ignore any explicitly-allowed names.
        if ignore_names.matches(name) {
            return None;
        }
        return Some(Diagnostic::new(
            InvalidArgumentName {
                name: name.to_string(),
            },
            parameter.range(),
        ));
    }
    None
}
