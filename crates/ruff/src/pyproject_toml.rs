use colored::Colorize;
use log::warn;
use pyproject_toml::{BuildSystem, Project};
use ruff_text_size::{TextRange, TextSize};
use serde::{Deserialize, Serialize};

use ruff_diagnostics::Diagnostic;
use ruff_source_file::SourceFile;

use crate::message::Message;
use crate::registry::Rule;
use crate::rules::ruff::rules::InvalidPyprojectToml;
use crate::settings::Settings;
use crate::IOError;

/// Unlike [`pyproject_toml::PyProjectToml`], in our case `build_system` is also optional
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct PyProjectToml {
    /// Build-related data
    build_system: Option<BuildSystem>,
    /// Project metadata
    project: Option<Project>,
}

pub fn lint_pyproject_toml(source_file: SourceFile, settings: &Settings) -> Vec<Message> {
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
                    let diagnostic = Diagnostic::new(IOError { message }, TextRange::default());
                    messages.push(Message::from_diagnostic(
                        diagnostic,
                        source_file,
                        TextSize::default(),
                    ));
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
        let diagnostic = Diagnostic::new(InvalidPyprojectToml { message: toml_err }, range);
        messages.push(Message::from_diagnostic(
            diagnostic,
            source_file,
            TextSize::default(),
        ));
    }

    messages
}
