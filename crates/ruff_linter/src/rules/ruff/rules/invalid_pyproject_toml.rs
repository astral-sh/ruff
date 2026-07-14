use colored::Colorize;
use log::warn;
use pyproject_toml::PyProjectToml;

use ruff_db::diagnostic::Diagnostic;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_source_file::SourceFile;
use ruff_text_size::{TextRange, TextSize};

use crate::{FixAvailability, IOError, Violation, codes::Rule, settings::LinterSettings};

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
pub(crate) fn invalid_pyproject_toml(
    source_file: &SourceFile,
    settings: &LinterSettings,
) -> Vec<Diagnostic> {
    let Some(err) = toml::from_str::<PyProjectToml>(source_file.source_text()).err() else {
        return Vec::default();
    };

    let mut messages = Vec::new();
    let range = match err.span() {
        // This is bad but sometimes toml and/or serde just don't give us spans
        // TODO(konstin,micha): https://github.com/astral-sh/ruff/issues/4571
        None => TextRange::default(),
        Some(range) => {
            let Ok(end) = TextSize::try_from(range.end) else {
                let message = format!(
                    "{} is larger than 4GB, but ruff assumes all files to be smaller",
                    source_file.name(),
                );
                if settings.rules.enabled(Rule::IOError) {
                    let diagnostic =
                        IOError { message }.into_diagnostic(TextRange::default(), source_file);
                    messages.push(diagnostic);
                } else {
                    warn!(
                        "{}{}{} {message}",
                        "Failed to lint ".bold(),
                        source_file.name().bold(),
                        ":".bold()
                    );
                }
                return messages;
            };
            TextRange::new(
                // start <= end, so if end < 4GB follows start < 4GB
                TextSize::try_from(range.start).unwrap(),
                end,
            )
        }
    };

    if settings.rules.enabled(Rule::InvalidPyprojectToml) {
        let toml_err = err.message().to_string();
        let diagnostic =
            InvalidPyprojectToml { message: toml_err }.into_diagnostic(range, source_file);
        messages.push(diagnostic);
    }

    messages
}
