use anyhow::Result;
use pyproject_toml::{BuildSystem, Project};
use ruff_text_size::{TextRange, TextSize};
use serde::{Deserialize, Serialize};

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::source_code::SourceFile;

use crate::message::Message;
use crate::rules::ruff::rules::InvalidPyprojectToml;
use crate::IOError;

/// Unlike [pyproject_toml::PyProjectToml], in our case `build_system` is also optional
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct PyProjectToml {
    /// Build-related data
    build_system: Option<BuildSystem>,
    /// Project metadata
    project: Option<Project>,
}

pub fn lint_pyproject_toml(source_file: SourceFile) -> Result<Vec<Message>> {
    let err = match toml::from_str::<PyProjectToml>(source_file.source_text()) {
        Ok(_) => return Ok(Vec::default()),
        Err(err) => err,
    };

    let range = match err.span() {
        // This is bad but sometimes toml and/or serde just don't give us spans
        // TODO(konstin,micha): https://github.com/charliermarsh/ruff/issues/4571
        None => TextRange::default(),
        Some(range) => {
            let end = match TextSize::try_from(range.end) {
                Ok(end) => end,
                Err(_) => {
                    let diagnostic = Diagnostic::new(
                        IOError {
                            message: format!("pyproject.toml is larger than 4GB"),
                        },
                        TextRange::default(),
                    );
                    return Ok(vec![Message::from_diagnostic(
                        diagnostic,
                        source_file,
                        TextSize::default(),
                    )]);
                }
            };
            TextRange::new(
                // start <= end, so if end < 4GB follows start < 4GB
                TextSize::try_from(range.start).unwrap(),
                end,
            )
        }
    };

    let toml_err = err.message().to_string();
    let diagnostic = Diagnostic::new(InvalidPyprojectToml { message: toml_err }, range);
    Ok(vec![Message::from_diagnostic(
        diagnostic,
        source_file,
        TextSize::default(),
    )])
}
