//! Settings for the `pylint` plugin.

use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::display_settings;
use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_macros::CacheKey;
use ruff_python_ast::{ExprNumberLiteral, LiteralExpressionRef, Number, UnaryOp};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, CacheKey)]
#[serde(untagged)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum AllowedValue {
    String(String),
    Int(i64),
    Float(AllowedFloatValue),
}

impl AllowedValue {
    pub(crate) fn matches_value(
        &self,
        literal_expr: LiteralExpressionRef<'_>,
        unary_op: Option<&UnaryOp>,
    ) -> bool {
        match (self, literal_expr) {
            (Self::String(allowed), LiteralExpressionRef::StringLiteral(string)) => {
                unary_op.is_none() && allowed == string.value.to_str()
            }
            (
                Self::Int(allowed),
                LiteralExpressionRef::NumberLiteral(ExprNumberLiteral {
                    value: Number::Int(value),
                    ..
                }),
            ) => {
                let Some(value) = value.as_i64() else {
                    return false;
                };
                let value = match unary_op {
                    None | Some(UnaryOp::UAdd) => value,
                    Some(UnaryOp::USub) => -value,
                    Some(UnaryOp::Invert | UnaryOp::Not) => return false,
                };
                value == *allowed
            }
            (
                Self::Float(allowed),
                LiteralExpressionRef::NumberLiteral(ExprNumberLiteral {
                    value: Number::Float(value),
                    ..
                }),
            ) => {
                let value = match unary_op {
                    None | Some(UnaryOp::UAdd) => *value,
                    Some(UnaryOp::USub) => -value,
                    Some(UnaryOp::Invert | UnaryOp::Not) => return false,
                };
                AllowedFloatValue(value) == *allowed
            }
            _ => false,
        }
    }
}

impl fmt::Display for AllowedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(value) => write!(f, "{value:?}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{:?}", value.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct AllowedFloatValue(f64);

impl PartialEq for AllowedFloatValue {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for AllowedFloatValue {}

impl CacheKey for AllowedFloatValue {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.0.to_bits().cache_key(state);
    }
}

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
    pub(crate) fn try_from_literal_expr(literal_expr: LiteralExpressionRef<'_>) -> Option<Self> {
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

impl fmt::Display for ConstantType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytes => write!(f, "bytes"),
            Self::Complex => write!(f, "complex"),
            Self::Float => write!(f, "float"),
            Self::Int => write!(f, "int"),
            Self::Str => write!(f, "str"),
        }
    }
}

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub allow_magic_value_types: Vec<ConstantType>,
    pub allow_magic_values: Vec<AllowedValue>,
    pub allow_dunder_method_names: FxHashSet<String>,
    pub max_args: usize,
    pub max_positional_args: usize,
    pub max_returns: usize,
    pub max_bool_expr: usize,
    pub max_branches: usize,
    pub max_statements: usize,
    pub max_statements_in_try: usize,
    pub max_public_methods: usize,
    pub max_locals: usize,
    pub max_nested_blocks: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_magic_value_types: vec![ConstantType::Str, ConstantType::Bytes],
            allow_magic_values: vec![
                AllowedValue::Int(0),
                AllowedValue::Int(1),
                AllowedValue::Int(-1),
                AllowedValue::Float(AllowedFloatValue(0.0)),
                AllowedValue::Float(AllowedFloatValue(1.0)),
                AllowedValue::Float(AllowedFloatValue(-1.0)),
                AllowedValue::String(String::new()),
                AllowedValue::String("__main__".to_string()),
            ],
            allow_dunder_method_names: FxHashSet::default(),
            max_args: 5,
            max_positional_args: 5,
            max_returns: 6,
            max_bool_expr: 5,
            max_branches: 12,
            max_statements: 50,
            max_statements_in_try: 5,
            max_public_methods: 20,
            max_locals: 15,
            max_nested_blocks: 5,
        }
    }
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.pylint",
            fields = [
                self.allow_magic_value_types | array,
                self.allow_magic_values | array,
                self.allow_dunder_method_names | set,
                self.max_args,
                self.max_positional_args,
                self.max_returns,
                self.max_bool_expr,
                self.max_branches,
                self.max_statements,
                self.max_statements_in_try,
                self.max_public_methods,
                self.max_locals,
                self.max_nested_blocks
            ]
        }
        Ok(())
    }
}
