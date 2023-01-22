//! Extract isort configuration settings from a pyproject.toml.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// The [isort configuration](https://pycqa.github.io/isort/docs/configuration/config_files.html).
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Isort {
    #[serde(alias = "src-paths", alias = "src_paths")]
    pub src_paths: Option<Vec<String>>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Tools {
    isort: Option<Isort>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Pyproject {
    tool: Option<Tools>,
}

pub fn parse_isort_options<P: AsRef<Path>>(path: P) -> Result<Option<Isort>> {
    let contents = std::fs::read_to_string(path)?;
    Ok(toml_edit::easy::from_str::<Pyproject>(&contents)?
        .tool
        .and_then(|tool| tool.isort))
}
