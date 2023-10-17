//! Extract Black configuration settings from a pyproject.toml.

use ruff_linter::line_width::LineWidth;
use ruff_linter::settings::types::PythonVersion;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct Black {
    #[serde(alias = "line-length", alias = "line_length")]
    pub(crate) line_length: Option<LineWidth>,
    #[serde(alias = "target-version", alias = "target_version")]
    pub(crate) target_version: Option<Vec<PythonVersion>>,
}
