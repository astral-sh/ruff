use colored::Colorize;
use log::warn;
use pyproject_toml::PyProjectToml;
use ruff_text_size::{TextRange, TextSize};

use ruff_db::diagnostic::Diagnostic;
use ruff_source_file::SourceFile;

use crate::registry::Rule;
use crate::rules::ruff::rules::InvalidPyprojectToml;
use crate::settings::LinterSettings;
use crate::{IOError, Violation};

/// RUF200
pub fn lint_pyproject_toml(source_file: &SourceFile, settings: &LinterSettings) -> Vec<Diagnostic> {
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
