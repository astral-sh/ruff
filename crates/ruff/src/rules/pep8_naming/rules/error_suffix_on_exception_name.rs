use rustpython_parser::ast::{Expr, ExprKind, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::identifier_range;
use ruff_python_ast::source_code::Locator;

/// ## What it does
/// Checks for custom exception definitions that omit the `Error` suffix.
///
/// ## Why is this bad?
/// The `Error` suffix is recommended by [PEP 8]:
///
/// > Because exceptions should be classes, the class naming convention
/// > applies here. However, you should use the suffix `"Error"` on your
/// > exception names (if the exception actually is an error).
///
/// ## Example
/// ```python
/// class Validation(Exception):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// class ValidationError(Exception):
///     ...
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#exception-names
#[violation]
pub struct ErrorSuffixOnExceptionName {
    pub name: String,
}

impl Violation for ErrorSuffixOnExceptionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ErrorSuffixOnExceptionName { name } = self;
        format!("Exception name `{name}` should be named with an Error suffix")
    }
}

/// N818
pub fn error_suffix_on_exception_name(
    class_def: &Stmt,
    bases: &[Expr],
    name: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if !bases.iter().any(|base| {
        if let ExprKind::Name { id, .. } = &base.node {
            id == "Exception" || id.ends_with("Error")
        } else {
            false
        }
    }) {
        return None;
    }

    if name.ends_with("Error") {
        return None;
    }
    Some(Diagnostic::new(
        ErrorSuffixOnExceptionName {
            name: name.to_string(),
        },
        identifier_range(class_def, locator),
    ))
}
