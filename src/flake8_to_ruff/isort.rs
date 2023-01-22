//! Extract isort configuration settings from a pyproject.toml.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

// use crate::settings::types::PythonVersion;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ISort {
    #[serde(alias = "src-paths", alias = "src_paths")]
    pub src_paths: Option<Vec<String>>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Tools {
    isort: Option<ISort>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Pyproject {
    tool: Option<Tools>,
}

pub fn parse_isort_options<P: AsRef<Path>>(path: P) -> Result<Option<ISort>> {
    let contents = std::fs::read_to_string(path)?;
    Ok(toml_edit::easy::from_str::<Pyproject>(&contents)?
        .tool
        .and_then(|tool| tool.isort))
}
