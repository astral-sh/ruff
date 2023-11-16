use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for variables defined in `for`, `try`, `with` statements
/// that redefine function parameters.
///
/// ## Why is this bad?
/// Redefined variable can cause unexpected behavior because of overridden function parameter.
/// If nested functions are declared, inner function's body can override outer function's parameter.
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
/// ## References
/// - [Pylint documentation](https://pylint.readthedocs.io/en/latest/user_guide/messages/refactor/redefined-argument-from-local.html)

#[violation]
pub struct RedefinedArgumentFromLocal {
    pub(crate) name: String,
}

impl Violation for RedefinedArgumentFromLocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedArgumentFromLocal { name } = self;
        format!("Redefining argument with the local name `{name}`")
    }
}
