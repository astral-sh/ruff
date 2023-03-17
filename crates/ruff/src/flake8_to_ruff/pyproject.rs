use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::black::Black;
use super::isort::Isort;
use super::pep621::Project;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tools {
    pub black: Option<Black>,
    pub isort: Option<Isort>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pyproject {
    pub tool: Option<Tools>,
    pub project: Option<Project>,
}

pub fn parse<P: AsRef<Path>>(path: P) -> Result<Pyproject> {
    let contents = std::fs::read_to_string(path)?;
    let pyproject = toml::from_str::<Pyproject>(&contents)?;
    Ok(pyproject)
}
