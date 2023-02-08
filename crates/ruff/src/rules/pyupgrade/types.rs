use rustpython_parser::ast::Constant;
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Primitive {
    Bool,
    Str,
    Bytes,
    Int,
    Float,
    Complex,
}

impl Primitive {
    pub const fn from_constant(constant: &Constant) -> Option<Self> {
        match constant {
            Constant::Bool(_) => Some(Self::Bool),
            Constant::Str(_) => Some(Self::Str),
            Constant::Bytes(_) => Some(Self::Bytes),
            Constant::Int(_) => Some(Self::Int),
            Constant::Float(_) => Some(Self::Float),
            Constant::Complex { .. } => Some(Self::Complex),
            _ => None,
        }
    }

    pub fn builtin(self) -> String {
        match self {
            Self::Bool => "bool".to_string(),
            Self::Str => "str".to_string(),
            Self::Bytes => "bytes".to_string(),
            Self::Int => "int".to_string(),
            Self::Float => "float".to_string(),
            Self::Complex => "complex".to_string(),
        }
    }
}
