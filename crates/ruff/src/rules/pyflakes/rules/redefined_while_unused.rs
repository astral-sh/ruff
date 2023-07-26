use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::OneIndexed;

/// ## What it does
/// Checks for variable definitions that redefine (or "shadow") unused
/// variables.
///
/// ## Why is this bad?
/// Redefinitions of unused names are unnecessary and often indicative of a
/// mistake.
///
/// ## Example
/// ```python
/// import foo
/// import bar
/// import foo  # Redefinition of unused `foo` from line 1
/// ```
///
/// Use instead:
/// ```python
/// import foo
/// import bar
/// ```
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
