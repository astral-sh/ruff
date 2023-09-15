//! Settings for the `pylint` plugin.

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use ruff_macros::CacheKey;
use ruff_python_ast::Constant;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ConstantType {
    Bytes,
    Complex,
    Float,
    Int,
    Str,
    Tuple,
}

impl TryFrom<&Constant> for ConstantType {
    type Error = anyhow::Error;

    fn try_from(value: &Constant) -> Result<Self, Self::Error> {
        match value {
            Constant::Bytes(..) => Ok(Self::Bytes),
            Constant::Complex { .. } => Ok(Self::Complex),
            Constant::Float(..) => Ok(Self::Float),
            Constant::Int(..) => Ok(Self::Int),
            Constant::Str(..) => Ok(Self::Str),
            Constant::Bool(..) | Constant::Ellipsis | Constant::None => {
                Err(anyhow!("Singleton constants are unsupported"))
            }
        }
    }
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub allow_magic_value_types: Vec<ConstantType>,
    pub max_args: usize,
    pub max_returns: usize,
    pub max_branches: usize,
    pub max_statements: usize,
    pub max_public_methods: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_magic_value_types: vec![ConstantType::Str, ConstantType::Bytes],
            max_args: 5,
            max_returns: 6,
            max_branches: 12,
            max_statements: 50,
            max_public_methods: 20,
        }
    }
}
