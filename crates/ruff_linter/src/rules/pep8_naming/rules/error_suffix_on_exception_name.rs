use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::identifier::Identifier;

use crate::rules::pep8_naming::settings::IgnoreNames;

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
///
/// ```python
/// class Validation(Exception): ...
/// ```
///
/// Use instead:
///
/// ```python
/// class ValidationError(Exception): ...
/// ```
///
/// ## Options
/// - `lint.pep8-naming.ignore-names`
/// - `lint.pep8-naming.extend-ignore-names`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#exception-names
#[derive(ViolationMetadata)]
pub(crate) struct ErrorSuffixOnExceptionName {
    name: String,
}

impl Violation for ErrorSuffixOnExceptionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ErrorSuffixOnExceptionName { name } = self;
        format!("Exception name `{name}` should be named with an Error suffix")
    }
}

/// N818
pub(crate) fn error_suffix_on_exception_name(
    class_def: &Stmt,
    arguments: Option<&Arguments>,
    name: &str,
    ignore_names: &IgnoreNames,
) -> Option<Diagnostic> {
    if name.ends_with("Error") {
        return None;
    }

    if !arguments.is_some_and(|arguments| {
        arguments.args.iter().any(|base| {
            if let Expr::Name(ast::ExprName { id, .. }) = &base {
                id == "Exception" || id.ends_with("Error")
            } else {
                false
            }
        })
    }) {
        return None;
    }

    // Ignore any explicitly-allowed names.
    if ignore_names.matches(name) {
        return None;
    }

    Some(Diagnostic::new(
        ErrorSuffixOnExceptionName {
            name: name.to_string(),
        },
        class_def.identifier(),
    ))
}
