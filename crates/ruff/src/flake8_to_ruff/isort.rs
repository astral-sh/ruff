//! Extract isort configuration settings from a pyproject.toml.

use serde::{Deserialize, Serialize};

/// The [isort configuration](https://pycqa.github.io/isort/docs/configuration/config_files.html).
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Isort {
    #[serde(alias = "force-single-line", alias = "force_single_line")]
    pub force_single_line: Option<bool>,
    #[serde(alias = "src-paths", alias = "src_paths")]
    pub src_paths: Option<Vec<String>>,
}
