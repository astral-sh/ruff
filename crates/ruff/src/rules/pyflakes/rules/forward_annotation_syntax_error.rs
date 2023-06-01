use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for forward annotations with invalid syntax.
///
/// ## Why is this bad?
/// Forward annotations with invalid syntax will not be parsed correctly.
///
/// ## Example
/// ```python
/// def foo() -> "/":
///     ...
/// ```
///
/// ## References
/// - [PEP 563](https://www.python.org/dev/peps/pep-0563/)
#[violation]
pub struct ForwardAnnotationSyntaxError {
    pub body: String,
}

impl Violation for ForwardAnnotationSyntaxError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ForwardAnnotationSyntaxError { body } = self;
        format!("Syntax error in forward annotation: `{body}`")
    }
}
