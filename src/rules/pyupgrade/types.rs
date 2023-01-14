use rustpython_ast::Constant;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Primitive {
    Bool,
    Str,
    Bytes,
    Int,
    Float,
    Complex,
}

impl Primitive {
    pub fn from_constant(constant: &Constant) -> Option<Self> {
        match constant {
            Constant::Bool(_) => Some(Primitive::Bool),
            Constant::Str(_) => Some(Primitive::Str),
            Constant::Bytes(_) => Some(Primitive::Bytes),
            Constant::Int(_) => Some(Primitive::Int),
            Constant::Float(_) => Some(Primitive::Float),
            Constant::Complex { .. } => Some(Primitive::Complex),
            _ => None,
        }
    }

    pub fn builtin(&self) -> String {
        match self {
            Primitive::Bool => "bool".to_string(),
            Primitive::Str => "str".to_string(),
            Primitive::Bytes => "bytes".to_string(),
            Primitive::Int => "int".to_string(),
            Primitive::Float => "float".to_string(),
            Primitive::Complex => "complex".to_string(),
        }
    }
}
