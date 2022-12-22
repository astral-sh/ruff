//! Extract Black configuration settings from a pyproject.toml.

use std::path::Path;

use anyhow::Result;
use ruff::settings::types::PythonVersion;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Black {
    #[serde(alias = "line-length", alias = "line_length")]
    pub line_length: Option<usize>,
    #[serde(alias = "target-version", alias = "target_version")]
    pub target_version: Option<Vec<PythonVersion>>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Tools {
    black: Option<Black>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Pyproject {
    tool: Option<Tools>,
}

pub fn parse_black_options<P: AsRef<Path>>(path: P) -> Result<Option<Black>> {
    let contents = std::fs::read_to_string(path)?;
    Ok(toml::from_str::<Pyproject>(&contents)?
        .tool
        .and_then(|tool| tool.black))
}
