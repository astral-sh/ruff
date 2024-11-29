use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

/// ## What it does
/// Checks for variables defined in `for`, `try`, `with` statements
/// that redefine function parameters.
///
/// ## Why is this bad?
/// Redefined variables can cause unexpected behavior because of overridden function parameters.
/// If nested functions are declared, an inner function's body can override an outer function's parameters.
///
/// ## Example
/// ```python
/// def show(host_id=10.11):
///     for host_id, host in [[12.13, "Venus"], [14.15, "Mars"]]:
///         print(host_id, host)
/// ```
///
/// Use instead:
/// ```python
/// def show(host_id=10.11):
///     for inner_host_id, host in [[12.13, "Venus"], [14.15, "Mars"]]:
///         print(host_id, inner_host_id, host)
/// ```
///
/// ## Options
/// - `lint.dummy-variable-rgx`
///
/// ## References
/// - [Pylint documentation](https://pylint.readthedocs.io/en/latest/user_guide/messages/refactor/redefined-argument-from-local.html)

#[derive(ViolationMetadata)]
pub(crate) struct RedefinedArgumentFromLocal {
    pub(crate) name: String,
}

impl Violation for RedefinedArgumentFromLocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedArgumentFromLocal { name } = self;
        format!("Redefining argument with the local name `{name}`")
    }
}
