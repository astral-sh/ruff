use ruff_python_ast::{Expr, Number, flat};
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
    pub(crate) fn from_expr(expr: flat::View<'_, Expr>) -> Option<Self> {
        match expr.classify() {
            flat::ExprKind::BooleanLiteral(_) => Some(Self::Bool),
            flat::ExprKind::StringLiteral(_) => Some(Self::Str),
            flat::ExprKind::BytesLiteral(_) => Some(Self::Bytes),
            flat::ExprKind::NumberLiteral(expr_number_literal) => {
                let value = expr_number_literal.value();
                match value {
                    Number::Int(_) => Some(Self::Int),
                    Number::Float(_) => Some(Self::Float),
                    Number::Complex { .. } => Some(Self::Complex),
                }
            }
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
