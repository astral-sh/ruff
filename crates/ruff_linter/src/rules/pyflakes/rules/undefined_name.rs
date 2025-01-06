use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

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
/// ## Options
/// - [`target-version`]: Can be used to configure which symbols Ruff will understand
///   as being available in the `builtins` namespace.
///
/// ## References
/// - [Python documentation: Naming and binding](https://docs.python.org/3/reference/executionmodel.html#naming-and-binding)
#[derive(ViolationMetadata)]
pub(crate) struct UndefinedName {
    pub(crate) name: String,
    pub(crate) minor_version_builtin_added: Option<u8>,
}

impl Violation for UndefinedName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedName {
            name,
            minor_version_builtin_added,
        } = self;
        let tip = minor_version_builtin_added.map(|version_added| {
            format!(
                r#"Consider specifying `requires-python = ">= 3.{version_added}"` or `tool.ruff.target-version = "py3{version_added}"` in your `pyproject.toml` file."#
            )
        });

        if let Some(tip) = tip {
            format!("Undefined name `{name}`. {tip}")
        } else {
            format!("Undefined name `{name}`")
        }
    }
}
