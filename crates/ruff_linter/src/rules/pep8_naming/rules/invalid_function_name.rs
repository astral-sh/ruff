use ruff_python_ast::{Decorator, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::str;

use crate::rules::pep8_naming::settings::IgnoreNames;

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
/// Names can be excluded from this rule using the [`lint.pep8-naming.ignore-names`]
/// or [`lint.pep8-naming.extend-ignore-names`] configuration options. For example,
/// to ignore all functions starting with `test_` from this rule, set the
/// [`lint.pep8-naming.extend-ignore-names`] option to `["test_*"]`.
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
/// ## Options
/// - `lint.pep8-naming.ignore-names`
/// - `lint.pep8-naming.extend-ignore-names`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-variable-names
#[derive(ViolationMetadata)]
pub(crate) struct InvalidFunctionName {
    name: String,
}

impl Violation for InvalidFunctionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidFunctionName { name } = self;
        format!("Function name `{name}` should be lowercase")
    }
}

/// N802
pub(crate) fn invalid_function_name(
    stmt: &Stmt,
    name: &str,
    decorator_list: &[Decorator],
    ignore_names: &IgnoreNames,
    semantic: &SemanticModel,
) -> Option<Diagnostic> {
    // Ignore any function names that are already lowercase.
    if str::is_lowercase(name) {
        return None;
    }

    // Ignore any functions that are explicitly `@override` or `@overload`.
    // These are defined elsewhere, so if they're first-party,
    // we'll flag them at the definition site.
    if visibility::is_override(decorator_list, semantic)
        || visibility::is_overload(decorator_list, semantic)
    {
        return None;
    }

    // Ignore any explicitly-allowed names.
    if ignore_names.matches(name) {
        return None;
    }

    Some(Diagnostic::new(
        InvalidFunctionName {
            name: name.to_string(),
        },
        stmt.identifier(),
    ))
}
