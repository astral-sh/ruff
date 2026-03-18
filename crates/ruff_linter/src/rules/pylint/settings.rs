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
    Int(i32),
    Float(AllowedFloatValue),
}

impl AllowedValue {
    pub fn matches_value(
        &self,
        literal_expr: LiteralExpressionRef<'_>,
        unary_op: Option<&UnaryOp>,
    ) -> bool {
        match (self, literal_expr) {
            (
                AllowedValue::String(allowed),
                LiteralExpressionRef::StringLiteral(string_literal),
            ) => unary_op.is_none() && allowed.as_str() == string_literal.value.to_str(),
            (AllowedValue::Int(allowed), LiteralExpressionRef::NumberLiteral(number_literal)) => {
                number_to_i32(&number_literal.value)
                    .and_then(|value| apply_unary_int(value, unary_op))
                    == Some(*allowed)
            }
            (AllowedValue::Float(allowed), LiteralExpressionRef::NumberLiteral(number_literal)) => {
                number_to_f64(&number_literal.value)
                    .and_then(|value| apply_unary_float(value, unary_op))
                    == Some(allowed.value())
            }
            _ => false,
        }
    }
}

fn number_to_i32(number: &Number) -> Option<i32> {
    match number {
        Number::Int(i) => i.as_i32(),
        #[expect(clippy::cast_possible_truncation)]
        Number::Float(f) if f.fract() == 0.0 => Some(*f as i32),
        Number::Float(_) | Number::Complex { .. } => None,
    }
}

fn number_to_f64(number: &Number) -> Option<f64> {
    match number {
        Number::Int(i) => i.as_i32().map(f64::from),
        Number::Float(f) => Some(*f),
        Number::Complex { .. } => None,
    }
}

fn apply_unary_int(value: i32, unary_op: Option<&UnaryOp>) -> Option<i32> {
    match unary_op {
        None | Some(UnaryOp::UAdd) => Some(value),
        Some(UnaryOp::USub) => value.checked_neg(),
        Some(UnaryOp::Invert | UnaryOp::Not) => None,
    }
}

fn apply_unary_float(value: f64, unary_op: Option<&UnaryOp>) -> Option<f64> {
    match unary_op {
        None | Some(UnaryOp::UAdd) => Some(value),
        Some(UnaryOp::USub) => Some(-value),
        Some(UnaryOp::Invert | UnaryOp::Not) => None,
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

impl CacheKey for AllowedFloatValue {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
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
                    write!(f, "{value:.?}")
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
