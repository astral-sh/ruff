use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::identifier_range;
use ruff_python_ast::source_code::Locator;

/// ## What it does
/// Checks for functions names that do not follow the `snake_case` naming
/// convention.
///
/// ## Why is this bad?
/// [PEP 8] recommends that function names follow `snake_case`:
///
/// > Function names should be lowercase, with words separated by underscores as necessary to
/// > improve readability. mixedCase is allowed only in contexts where that’s already the
/// > prevailing style (e.g. threading.py), to retain backwards compatibility.
///
/// ## Options
/// - `pep8-naming.ignore-names`
///
/// ## Example
/// ```python
/// def myFunction():
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def my_function():
///     pass
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-variable-names
#[violation]
pub struct InvalidFunctionName {
    pub name: String,
}

impl Violation for InvalidFunctionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidFunctionName { name } = self;
        format!("Function name `{name}` should be lowercase")
    }
}

/// N802
pub fn invalid_function_name(
    func_def: &Stmt,
    name: &str,
    ignore_names: &[String],
    locator: &Locator,
) -> Option<Diagnostic> {
    if ignore_names.iter().any(|ignore_name| ignore_name == name) {
        return None;
    }
    if name.to_lowercase() != name {
        return Some(Diagnostic::new(
            InvalidFunctionName {
                name: name.to_string(),
            },
            identifier_range(func_def, locator),
        ));
    }
    None
}
