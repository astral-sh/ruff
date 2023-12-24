//! Settings for the `pylint` plugin.

use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};

use ruff_macros::CacheKey;
use ruff_python_ast::{ExprNumberLiteral, LiteralExpressionRef, Number};

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

impl ConstantType {
    pub fn try_from_literal_expr(literal_expr: LiteralExpressionRef<'_>) -> Option<Self> {
        match literal_expr {
            LiteralExpressionRef::StringLiteral(_) => Some(Self::Str),
            LiteralExpressionRef::BytesLiteral(_) => Some(Self::Bytes),
            LiteralExpressionRef::NumberLiteral(ExprNumberLiteral { value, .. }) => match value {
                Number::Int(_) => Some(Self::Int),
                Number::Float(_) => Some(Self::Float),
                Number::Complex { .. } => Some(Self::Complex),
            },
            LiteralExpressionRef::BooleanLiteral(_)
            | LiteralExpressionRef::NoneLiteral(_)
            | LiteralExpressionRef::EllipsisLiteral(_) => None,
        }
    }
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub allow_magic_value_types: Vec<ConstantType>,
    pub allow_dunder_method_names: FxHashSet<String>,
    pub max_args: usize,
    pub max_positional_args: usize,
    pub max_returns: usize,
    pub max_bool_expr: usize,
    pub max_branches: usize,
    pub max_statements: usize,
    pub max_public_methods: usize,
    pub max_locals: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_magic_value_types: vec![ConstantType::Str, ConstantType::Bytes],
            allow_dunder_method_names: FxHashSet::default(),
            max_args: 5,
            max_positional_args: 5,
            max_returns: 6,
            max_bool_expr: 5,
            max_branches: 12,
            max_statements: 50,
            max_public_methods: 20,
            max_locals: 15,
        }
    }
}
