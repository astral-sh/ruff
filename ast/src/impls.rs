use crate::{Constant, ExprKind};

impl<U> ExprKind<U> {
    /// Returns a short name for the node suitable for use in error messages.
    pub fn name(&self) -> &'static str {
        match self {
            ExprKind::BoolOp { .. } | ExprKind::BinOp { .. } | ExprKind::UnaryOp { .. } => {
                "operator"
            }
            ExprKind::Subscript { .. } => "subscript",
            ExprKind::Await { .. } => "await expression",
            ExprKind::Yield { .. } | ExprKind::YieldFrom { .. } => "yield expression",
            ExprKind::Compare { .. } => "comparison",
            ExprKind::Attribute { .. } => "attribute",
            ExprKind::Call { .. } => "function call",
            ExprKind::Constant { value, .. } => match value {
                Constant::Str(_)
                | Constant::Int(_)
                | Constant::Float(_)
                | Constant::Complex { .. }
                | Constant::Bytes(_) => "literal",
                Constant::Tuple(_) => "tuple",
                Constant::Bool(b) => {
                    if *b {
                        "True"
                    } else {
                        "False"
                    }
                }
                Constant::None => "None",
                Constant::Ellipsis => "ellipsis",
            },
            ExprKind::List { .. } => "list",
            ExprKind::Tuple { .. } => "tuple",
            ExprKind::Dict { .. } => "dict display",
            ExprKind::Set { .. } => "set display",
            ExprKind::ListComp { .. } => "list comprehension",
            ExprKind::DictComp { .. } => "dict comprehension",
            ExprKind::SetComp { .. } => "set comprehension",
            ExprKind::GeneratorExp { .. } => "generator expression",
            ExprKind::Starred { .. } => "starred",
            ExprKind::Slice { .. } => "slice",
            ExprKind::JoinedStr { values } => {
                if values
                    .iter()
                    .any(|e| matches!(e.node, ExprKind::JoinedStr { .. }))
                {
                    "f-string expression"
                } else {
                    "literal"
                }
            }
            ExprKind::FormattedValue { .. } => "f-string expression",
            ExprKind::Name { .. } => "name",
            ExprKind::Lambda { .. } => "lambda",
            ExprKind::IfExp { .. } => "conditional expression",
            ExprKind::NamedExpr { .. } => "named expression",
        }
    }
}
