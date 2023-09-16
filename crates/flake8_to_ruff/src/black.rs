//! Extract Black configuration settings from a pyproject.toml.

use ruff::line_width::LineLength;
use ruff::settings::types::PythonVersion;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct Black {
    #[serde(alias = "line-length", alias = "line_length")]
    pub(crate) line_length: Option<LineLength>,
    #[serde(alias = "target-version", alias = "target_version")]
    pub(crate) target_version: Option<Vec<PythonVersion>>,
}
