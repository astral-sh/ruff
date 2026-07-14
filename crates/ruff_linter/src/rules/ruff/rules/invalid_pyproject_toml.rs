use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::{TextRange, TextSize};

use crate::{FixAvailability, Violation, checkers::ast::LintContext};

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
///   { name = "Ferris the Crab", email = "ferris@example.org" }
/// ]
/// ```
///
/// ## References
/// - [Specification of `[project]` in pyproject.toml](https://packaging.python.org/en/latest/specifications/declaring-project-metadata/)
/// - [Specification of `[build-system]` in pyproject.toml](https://peps.python.org/pep-0518/)
/// - [Draft but implemented license declaration extensions](https://peps.python.org/pep-0639)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.271")]
pub(crate) struct InvalidPyprojectToml {
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

/// RUF200
pub(crate) fn invalid_pyproject_toml(context: &LintContext, err: &toml::de::Error) {
    let range = match err.span() {
        // This is bad but sometimes toml and/or serde just don't give us spans
        // TODO(konstin,micha): https://github.com/astral-sh/ruff/issues/4571
        None => TextRange::default(),
        Some(range) => TextRange::new(
            TextSize::try_from(range.start).unwrap(),
            TextSize::try_from(range.end).unwrap(),
        ),
    };

    let toml_err = err.message().to_string();
    context.report_diagnostic(InvalidPyprojectToml { message: toml_err }, range);
}
