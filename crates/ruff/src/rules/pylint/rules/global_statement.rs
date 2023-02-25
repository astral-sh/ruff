use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::BindingKind;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use crate::Range;

define_violation!(
    /// ## What it does
    /// Checks for usage of the `global` statement to update a global identifier.
    ///
    /// ## Why is this bad?
    /// Global variables should be avoided unless very necessary because of these non-exhaustive
    /// list of reasons: it breaks encapsulation and makes tracing program flow very difficult.
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

// PLW0603
pub fn global_statement(checker: &mut Checker, name: &str) {
    let scope = &checker.scopes[*checker.scope_stack.last().expect("No current scope found")];

    if let Some(&bidx) = scope.bindings.get(name) {
        let binding = &checker.bindings[bidx];
        if BindingKind::is_global(&binding.kind) {
            let diag = Diagnostic::new(
                GlobalStatement {
                    name: name.to_string(),
                },
                Range::from_located(
                    // NOTE: This could've been `binding.range` except pylint wants to report the
                    // location of the `global` keyword instead of the identifier.
                    binding
                        .source
                        .as_ref()
                        .expect("Global statements should always have `source`"),
                ),
            );
            checker.diagnostics.push(diag);
        }
    }
}
