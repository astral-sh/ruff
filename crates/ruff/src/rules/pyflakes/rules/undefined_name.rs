use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for uses of undefined names.
///
/// ## Why is this bad?
/// An undefined name is likely to raise `NameError` at runtime.
///
/// ## Example
/// ```python
/// def double():
///     return n * 2  # raises `NameError` if `n` is undefined when `double` is called
/// ```
///
/// Use instead:
/// ```python
/// def double(n):
///     return n * 2
/// ```
///
/// ## References
/// - [Python documentation: Naming and binding](https://docs.python.org/3/reference/executionmodel.html#naming-and-binding)
#[violation]
pub struct UndefinedName {
    pub(crate) name: String,
}

impl Violation for UndefinedName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedName { name } = self;
        format!("Undefined name `{name}`")
    }
}
