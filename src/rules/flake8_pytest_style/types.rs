use std::fmt::{Display, Formatter};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ParametrizeNameType {
    #[serde(rename = "csv")]
    Csv,
    #[serde(rename = "tuple")]
    Tuple,
    #[serde(rename = "list")]
    List,
}

impl Default for ParametrizeNameType {
    fn default() -> Self {
        Self::Tuple
    }
}

impl Display for ParametrizeNameType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Csv => write!(f, "csv"),
            Self::Tuple => write!(f, "tuple"),
            Self::List => write!(f, "list"),
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ParametrizeValuesType {
    #[serde(rename = "tuple")]
    Tuple,
    #[serde(rename = "list")]
    List,
}

impl Default for ParametrizeValuesType {
    fn default() -> Self {
        Self::List
    }
}

impl Display for ParametrizeValuesType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tuple => write!(f, "tuple"),
            Self::List => write!(f, "list"),
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ParametrizeValuesRowType {
    #[serde(rename = "tuple")]
    Tuple,
    #[serde(rename = "list")]
    List,
}

impl Default for ParametrizeValuesRowType {
    fn default() -> Self {
        Self::Tuple
    }
}

impl Display for ParametrizeValuesRowType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tuple => write!(f, "tuple"),
            Self::List => write!(f, "list"),
        }
    }
}
