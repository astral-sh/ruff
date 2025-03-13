use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_semantic::analyze::type_inference::{PythonType, ResolvedPythonType};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `os.getenv` calls with invalid default values.
///
/// ## Why is this bad?
/// If an environment variable is set, `os.getenv` will return its value as
/// a string. If the environment variable is _not_ set, `os.getenv` will
/// return `None`, or the default value if one is provided.
///
/// If the default value is not a string or `None`, then it will be
/// inconsistent with the return type of `os.getenv`, which can lead to
/// confusing behavior.
///
/// ## Example
/// ```python
/// import os
///
/// int(os.getenv("FOO", 1))
/// ```
///
/// Use instead:
/// ```python
/// import os
///
/// int(os.getenv("FOO", "1"))
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct InvalidEnvvarDefault;

impl Violation for InvalidEnvvarDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid type for environment variable default; expected `str` or `None`".to_string()
    }
}

/// PLW1508
pub(crate) fn invalid_envvar_default(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::OS) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["os", "getenv"] | ["os", "environ", "get"]
            )
        })
    {
        // Find the `default` argument, if it exists.
        let Some(expr) = call.arguments.find_argument_value("default", 1) else {
            return;
        };

        if matches!(
            ResolvedPythonType::from(expr),
            ResolvedPythonType::Unknown
                | ResolvedPythonType::Atom(PythonType::String | PythonType::None)
        ) {
            return;
        }
        checker.report_diagnostic(Diagnostic::new(InvalidEnvvarDefault, expr.range()));
    }
}
