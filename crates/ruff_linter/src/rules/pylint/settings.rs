//! Settings for the `pylint` plugin.

use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::display_settings;
use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_macros::CacheKey;
use ruff_python_ast::{ExprNumberLiteral, ExprStringLiteral, LiteralExpressionRef, Number};
use std::hash::Hasher;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, CacheKey)]
#[serde(untagged)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum AllowedValue {
    String(String),
    Int(i64),
    Float(AllowedFloatValue),
}

impl AllowedValue {
    pub fn matches(
        literal_expr: LiteralExpressionRef<'_>,
        allowed_values: &[AllowedValue],
    ) -> bool {
        match literal_expr {
            LiteralExpressionRef::StringLiteral(ExprStringLiteral { value, .. }) => {
                let string_value = value.to_str();
                allowed_values.iter().any(|allowed| {
                    if let AllowedValue::String(s) = allowed {
                        s.as_str() == string_value
                    } else {
                        false
                    }
                })
            }
            LiteralExpressionRef::NumberLiteral(ExprNumberLiteral { value, .. }) => match value {
                Number::Int(i) => {
                    if let Some(int_value) = i.as_i64() {
                        allowed_values.iter().any(|allowed| {
                            if let AllowedValue::Int(allowed_int) = allowed {
                                *allowed_int == int_value
                            } else {
                                false
                            }
                        })
                    } else {
                        false
                    }
                }
                Number::Float(f) => {
                    let float_value = AllowedFloatValue::new(*f);
                    allowed_values.iter().any(|allowed| {
                        if let AllowedValue::Float(allowed_float) = allowed {
                            *allowed_float == float_value
                        } else {
                            false
                        }
                    })
                }
                Number::Complex { .. } => false,
            },
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct AllowedFloatValue(f64);

impl AllowedFloatValue {
    pub fn new(value: f64) -> Self {
        Self(value)
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

impl PartialEq for AllowedFloatValue {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for AllowedFloatValue {}

impl From<f64> for AllowedFloatValue {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

impl From<AllowedFloatValue> for f64 {
    fn from(value: AllowedFloatValue) -> Self {
        value.0
    }
}

impl CacheKey for AllowedFloatValue {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_usize(2);
        self.0.to_bits().cache_key(state);
    }
}

impl fmt::Display for AllowedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllowedValue::String(s) => write!(f, "\"{s}\""),
            AllowedValue::Int(i) => write!(f, "{i}"),
            AllowedValue::Float(fl) => {
                let value = fl.value();
                // Ensure floats always display with decimal point
                if value.fract() == 0.0 {
                    write!(f, "{value:.1}")
                } else {
                    write!(f, "{value}")
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
                AllowedValue::Float(AllowedFloatValue::new(0.0)),
                AllowedValue::Float(AllowedFloatValue::new(1.0)),
                AllowedValue::Float(AllowedFloatValue::new(-1.0)),
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
