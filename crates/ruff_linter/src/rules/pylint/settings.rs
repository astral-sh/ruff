//! Settings for the `pylint` plugin.

use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::display_settings;
use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_macros::CacheKey;
use ruff_python_ast::{ExprNumberLiteral, ExprStringLiteral, LiteralExpressionRef, Number};
use std::hash::Hasher;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum AllowedValue {
    String(String),
    Int(i64),
    Float(f64),
}

impl AllowedValue {
    pub fn try_from_literal_expr(literal_expr: LiteralExpressionRef<'_>) -> Option<Self> {
        match literal_expr {
            LiteralExpressionRef::StringLiteral(ExprStringLiteral { value, .. }) => {
                Some(AllowedValue::String(value.to_str().to_string()))
            }
            LiteralExpressionRef::NumberLiteral(ExprNumberLiteral { value, .. }) => match value {
                Number::Float(f) => Some(AllowedValue::Float(*f)),
                Number::Int(i) => i.as_i64().map(AllowedValue::Int),
                Number::Complex { .. } => None,
            },
            _ => None,
        }
    }
}

impl PartialEq for AllowedValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AllowedValue::String(a), AllowedValue::String(b)) => a == b,
            (AllowedValue::Int(a), AllowedValue::Int(b)) => a == b,
            // dealing with floating point precision issues
            (AllowedValue::Float(a), AllowedValue::Float(b)) => a.to_bits() == b.to_bits(),
            _ => false,
        }
    }
}

impl Eq for AllowedValue {}

impl CacheKey for AllowedValue {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        match self {
            AllowedValue::String(s) => {
                state.write_usize(0);
                s.cache_key(state);
            }
            AllowedValue::Int(i) => {
                state.write_usize(1);
                i.cache_key(state);
            }
            // dealing with floating point precision issues for deterministic caching
            AllowedValue::Float(f) => {
                state.write_usize(2);
                f.to_bits().cache_key(state);
            }
        }
    }
}

impl fmt::Display for AllowedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllowedValue::String(s) => write!(f, "\"{s}\""),
            AllowedValue::Int(i) => write!(f, "{i}"),
            AllowedValue::Float(fl) => {
                // Ensure floats always display with decimal point
                if fl.fract() == 0.0 {
                    write!(f, "{fl:.1}")
                } else {
                    write!(f, "{fl}")
                }
            }
        }
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
                AllowedValue::Float(0.0),
                AllowedValue::Float(1.0),
                AllowedValue::Float(-1.0),
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
                self.max_public_methods,
                self.max_locals,
                self.max_nested_blocks
            ]
        }
        Ok(())
    }
}
