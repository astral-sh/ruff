use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

/// ## What it does
///
/// Checks for syntax errors caused by using new Python features on old versions.
///
/// ## Why is this bad?
///
/// Such usage will cause a `SyntaxError` at runtime.
///
/// ## Example
///
/// The `match` statement was added to Python in version 3.10, so using it on version 3.9 or earlier
/// is an error.
///
/// ```python
/// match var:
///     case 1:
///         print("it's 1")
/// ```
///
/// To fix the issue, either configure your [`target-version`] to a newer Python version or avoid
/// the new syntax.
#[derive(ViolationMetadata)]
pub struct VersionSyntaxError {
    pub message: String,
}

impl Violation for VersionSyntaxError {
    #[allow(clippy::useless_format)]
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("{}", self.message)
    }
}
