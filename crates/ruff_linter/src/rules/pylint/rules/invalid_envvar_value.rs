use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::analyze::type_inference::{PythonType, ResolvedPythonType};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `os.getenv` calls with an invalid `key` argument.
///
/// ## Why is this bad?
/// `os.getenv` only supports strings as the first argument (`key`).
///
/// If the provided argument is not a string, `os.getenv` will throw a
/// `TypeError` at runtime.
///
/// ## Example
/// ```python
/// os.getenv(1)
/// ```
///
/// Use instead:
/// ```python
/// os.getenv("1")
/// ```
#[violation]
pub struct InvalidEnvvarValue;

impl Violation for InvalidEnvvarValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid type for initial `os.getenv` argument; expected `str`")
    }
}

/// PLE1507
pub(crate) fn invalid_envvar_value(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::OS) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["os", "getenv"]))
    {
        // Find the `key` argument, if it exists.
        let Some(expr) = call.arguments.find_argument("key", 0) else {
            return;
        };

        if matches!(
            ResolvedPythonType::from(expr),
            ResolvedPythonType::Unknown | ResolvedPythonType::Atom(PythonType::String)
        ) {
            return;
        }

        checker
            .diagnostics
            .push(Diagnostic::new(InvalidEnvvarValue, expr.range()));
    }
}
