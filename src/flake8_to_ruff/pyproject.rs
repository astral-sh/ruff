use serde::{Deserialize, Serialize};

use super::{black::Black, isort::Isort};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tools {
    pub black: Option<Black>,
    pub isort: Option<Isort>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pyproject {
    pub tool: Option<Tools>,
}
