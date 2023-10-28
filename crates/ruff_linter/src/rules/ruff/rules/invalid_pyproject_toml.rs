use ruff_diagnostics::{FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for any pyproject.toml that does not conform to the schema from the relevant PEPs.
///
/// ## Why is this bad?
/// Your project may contain invalid metadata or configuration without you noticing
///
/// ## Example
/// ```toml
/// [project]
/// name = "crab"
/// version = "1.0.0"
/// authors = ["Ferris the Crab <ferris@example.org>"]
/// ```
///
/// Use instead:
/// ```toml
/// [project]
/// name = "crab"
/// version = "1.0.0"
/// authors = [
///   { email = "ferris@example.org" },
///   { name = "Ferris the Crab"}
/// ]
/// ```
///
/// ## References
/// - [Specification of `[project]` in pyproject.toml](https://packaging.python.org/en/latest/specifications/declaring-project-metadata/)
/// - [Specification of `[build-system]` in pyproject.toml](https://peps.python.org/pep-0518/)
/// - [Draft but implemented license declaration extensions](https://peps.python.org/pep-0639)
#[violation]
pub struct InvalidPyprojectToml {
    pub message: String,
}

impl Violation for InvalidPyprojectToml {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidPyprojectToml { message } = self;
        format!("Failed to parse pyproject.toml: {message}")
    }
}
