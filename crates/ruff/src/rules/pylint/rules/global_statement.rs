use ruff_macros::{define_violation, derive_message_formats};

use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use crate::Range;

define_violation!(
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
    /// def foo():
    ///     global var  # [global-statement]
    ///     var = 10
    ///     print(var)
    ///
    /// foo()
    /// print(var)
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// var = 1
    ///
    /// def foo():
    ///     print(var)
    ///     return 10
    ///
    /// var = foo()
    /// print(var)
    /// ```
    pub struct GlobalStatement {
        pub name: String,
    }
);
impl Violation for GlobalStatement {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalStatement { name } = self;
        format!("Using the global statement to update `{name}` is discouraged")
    }
}

/// PLW0603
pub fn global_statement(checker: &mut Checker, name: &str) {
    let scope = checker.current_scope();
    if let Some(index) = scope.bindings.get(name) {
        let binding = &checker.bindings[*index];
        if binding.kind.is_global() {
            let diagnostic = Diagnostic::new(
                GlobalStatement {
                    name: name.to_string(),
                },
                // Match Pylint's behavior by reporting on the `global` statement`, rather
                // than the variable usage.
                Range::from_located(
                    binding
                        .source
                        .as_ref()
                        .expect("`global` bindings should always have a `source`"),
                ),
            );
            checker.diagnostics.push(diagnostic);
        }
    }
}
