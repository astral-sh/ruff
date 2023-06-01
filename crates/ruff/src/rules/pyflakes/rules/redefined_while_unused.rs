use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::OneIndexed;

/// ## What it does
/// Checks for redefinitions of unused names.
///
/// ## Why is this bad?
/// Redefinitions of unused names are unnecessary and indicative of a mistake.
///
/// ## Example
/// ```python
/// import foo
/// import bar
/// import foo  # redefinition of unused `foo`
/// ```
///
/// Use instead:
/// ```python
/// import foo
/// import bar
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/executionmodel.html#naming-and-binding)
#[violation]
pub struct RedefinedWhileUnused {
    pub name: String,
    pub line: OneIndexed,
}

impl Violation for RedefinedWhileUnused {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedWhileUnused { name, line } = self;
        format!("Redefinition of unused `{name}` from line {line}")
    }
}
