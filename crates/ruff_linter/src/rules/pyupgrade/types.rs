use ruff_python_ast::{Expr, ExprNumberLiteral, Number};
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Primitive {
    Bool,
    Str,
    Bytes,
    Int,
    Float,
    Complex,
}

impl Primitive {
    pub(crate) const fn from_expr(expr: &Expr) -> Option<Self> {
        match expr {
            Expr::BooleanLiteral(_) => Some(Self::Bool),
            Expr::StringLiteral(_) => Some(Self::Str),
            Expr::BytesLiteral(_) => Some(Self::Bytes),
            Expr::NumberLiteral(ExprNumberLiteral { value, .. }) => match value {
                Number::Int(_) => Some(Self::Int),
                Number::Float(_) => Some(Self::Float),
                Number::Complex { .. } => Some(Self::Complex),
            },
            _ => None,
        }
    }

    pub(crate) fn builtin(self) -> String {
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
