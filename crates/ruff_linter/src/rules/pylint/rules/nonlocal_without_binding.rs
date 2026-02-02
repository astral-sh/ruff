use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;

/// ## What it does
/// Checks for `nonlocal` names without bindings.
///
/// ## Why is this bad?
/// `nonlocal` names must be bound to a name in an outer scope.
/// Violating this rule leads to a `SyntaxError` at runtime.
///
/// ## Example
/// ```python
/// def foo():
///     def get_bar(self):
///         nonlocal bar
///         ...
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     bar = 1
///
///     def get_bar(self):
///         nonlocal bar
///         ...
/// ```
///
/// ## References
/// - [Python documentation: The `nonlocal` statement](https://docs.python.org/3/reference/simple_stmts.html#nonlocal)
/// - [PEP 3104 – Access to Names in Outer Scopes](https://peps.python.org/pep-3104/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.174")]
pub(crate) struct NonlocalWithoutBinding {
    pub(crate) name: String,
}

impl Violation for NonlocalWithoutBinding {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonlocalWithoutBinding { name } = self;
        format!("Nonlocal name `{name}` found without binding")
    }
}
