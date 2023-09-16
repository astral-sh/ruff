use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::black::Black;
use super::isort::Isort;
use super::pep621::Project;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Tools {
    pub(crate) black: Option<Black>,
    pub(crate) isort: Option<Isort>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Pyproject {
    pub(crate) tool: Option<Tools>,
    pub(crate) project: Option<Project>,
}

pub(crate) fn parse<P: AsRef<Path>>(path: P) -> Result<Pyproject> {
    let contents = std::fs::read_to_string(path)?;
    let pyproject = toml::from_str::<Pyproject>(&contents)?;
    Ok(pyproject)
}
