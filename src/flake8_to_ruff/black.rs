//! Extract Black configuration settings from a pyproject.toml.

use serde::{Deserialize, Serialize};

use crate::settings::types::PythonVersion;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Black {
    #[serde(alias = "line-length", alias = "line_length")]
    pub line_length: Option<usize>,
    #[serde(alias = "target-version", alias = "target_version")]
    pub target_version: Option<Vec<PythonVersion>>,
}
