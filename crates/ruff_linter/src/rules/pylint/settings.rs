//! Settings for the `pylint` plugin.

use serde::{Deserialize, Serialize};

use ruff_macros::CacheKey;
use ruff_python_ast::{Expr, ExprNumberLiteral, Number};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ConstantType {
    Bytes,
    Complex,
    Float,
    Int,
    Str,
}

impl TryFrom<&Expr> for ConstantType {
    type Error = ();

    fn try_from(value: &Expr) -> Result<Self, Self::Error> {
        match value {
            Expr::StringLiteral(_) => Ok(Self::Str),
            Expr::BytesLiteral(_) => Ok(Self::Bytes),
            Expr::NumberLiteral(ExprNumberLiteral { value, .. }) => match value {
                Number::Int(_) => Ok(Self::Int),
                Number::Float(_) => Ok(Self::Float),
                Number::Complex { .. } => Ok(Self::Complex),
            },
            _ => Err(()),
        }
    }
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub allow_magic_value_types: Vec<ConstantType>,
    pub max_args: usize,
    pub max_returns: usize,
    pub max_bool_expr: usize,
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
            max_bool_expr: 5,
            max_branches: 12,
            max_statements: 50,
            max_public_methods: 20,
        }
    }
}
