use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for forward annotations that include invalid syntax.
///
///
/// ## Why is this bad?
/// In Python, type annotations can be quoted as strings literals to enable
/// references to types that have not yet been defined, known as "forward
/// references".
///
/// However, these quoted annotations must be valid Python expressions. The use
/// of invalid syntax in a quoted annotation won't raise a `SyntaxError`, but
/// will instead raise an error when type checking is performed.
///
/// ## Example
///
/// ```python
/// def foo() -> "/": ...
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
