use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of `global` statements to update identifiers.
///
/// ## Why is this bad?
/// Pylint discourages the use of `global` variables as global mutable
/// state is a common source of bugs and confusing behavior.
///
/// ## Example
/// ```python
/// var = 1
///
///
/// def foo():
///     global var  # [global-statement]
///     var = 10
///     print(var)
///
///
/// foo()
/// print(var)
/// ```
///
/// Use instead:
/// ```python
/// var = 1
///
///
/// def foo():
///     var = 10
///     print(var)
///     return var
///
///
/// var = foo()
/// print(var)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct GlobalStatement {
    name: String,
}

impl Violation for GlobalStatement {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalStatement { name } = self;
        format!("Using the global statement to update `{name}` is discouraged")
    }
}

/// PLW0603
pub(crate) fn global_statement(checker: &Checker, name: &str) {
    if let Some(range) = checker.semantic().global(name) {
        checker.report_diagnostic(Diagnostic::new(
            GlobalStatement {
                name: name.to_string(),
            },
            // Match Pylint's behavior by reporting on the `global` statement`, rather
            // than the variable usage.
            range,
        ));
    }
}
