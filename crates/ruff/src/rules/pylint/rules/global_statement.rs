use rustpython_parser::ast::Ranged;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

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
///     print(var)
///     return 10
///
///
/// var = foo()
/// print(var)
/// ```
#[violation]
pub struct GlobalStatement {
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
pub(crate) fn global_statement(checker: &mut Checker, name: &str) {
    let scope = checker.semantic_model().scope();
    if let Some(binding_id) = scope.get(name) {
        let binding = &checker.semantic_model().bindings[binding_id];
        if binding.kind.is_global() {
            let source = checker.semantic_model().stmts[binding
                .source
                .expect("`global` bindings should always have a `source`")];
            let diagnostic = Diagnostic::new(
                GlobalStatement {
                    name: name.to_string(),
                },
                // Match Pylint's behavior by reporting on the `global` statement`, rather
                // than the variable usage.
                source.range(),
            );
            checker.diagnostics.push(diagnostic);
        }
    }
}
