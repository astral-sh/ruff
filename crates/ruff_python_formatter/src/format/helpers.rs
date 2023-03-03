use crate::cst::{Expr, ExprKind, UnaryOpKind};

pub fn is_self_closing(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Tuple { .. }
        | ExprKind::List { .. }
        | ExprKind::Set { .. }
        | ExprKind::Dict { .. }
        | ExprKind::ListComp { .. }
        | ExprKind::SetComp { .. }
        | ExprKind::DictComp { .. }
        | ExprKind::GeneratorExp { .. }
        | ExprKind::Call { .. }
        | ExprKind::Name { .. }
        | ExprKind::Constant { .. }
        | ExprKind::Subscript { .. } => true,
        ExprKind::Lambda { body, .. } => is_self_closing(body),
        ExprKind::BinOp { left, right, .. } => {
            matches!(left.node, ExprKind::Constant { .. } | ExprKind::Name { .. })
                && matches!(
                    right.node,
                    ExprKind::Tuple { .. }
                        | ExprKind::List { .. }
                        | ExprKind::Set { .. }
                        | ExprKind::Dict { .. }
                        | ExprKind::ListComp { .. }
                        | ExprKind::SetComp { .. }
                        | ExprKind::DictComp { .. }
                        | ExprKind::GeneratorExp { .. }
                        | ExprKind::Call { .. }
                        | ExprKind::Subscript { .. }
                )
        }
        ExprKind::BoolOp { values, .. } => values.last().map_or(false, |expr| {
            matches!(
                expr.node,
                ExprKind::Tuple { .. }
                    | ExprKind::List { .. }
                    | ExprKind::Set { .. }
                    | ExprKind::Dict { .. }
                    | ExprKind::ListComp { .. }
                    | ExprKind::SetComp { .. }
                    | ExprKind::DictComp { .. }
                    | ExprKind::GeneratorExp { .. }
                    | ExprKind::Call { .. }
                    | ExprKind::Subscript { .. }
            )
        }),
        ExprKind::UnaryOp { operand, .. } => is_self_closing(operand),
        _ => false,
    }
}

/// Return `true` if an [`Expr`] adheres to Black's definition of a non-complex
/// expression, in the context of a slice operation.
pub fn is_simple_slice(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::UnaryOp { op, operand } => {
            if matches!(op.node, UnaryOpKind::Not) {
                false
            } else {
                is_simple_slice(operand)
            }
        }
        ExprKind::Constant { .. } => true,
        ExprKind::Name { .. } => true,
        _ => false,
    }
}

/// Return `true` if an [`Expr`] adheres to Black's definition of a non-complex
/// expression, in the context of a power operation.
pub fn is_simple_power(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::UnaryOp { op, operand } => {
            if matches!(op.node, UnaryOpKind::Not) {
                false
            } else {
                is_simple_slice(operand)
            }
        }
        ExprKind::Constant { .. } => true,
        ExprKind::Name { .. } => true,
        ExprKind::Attribute { value, .. } => is_simple_power(value),
        _ => false,
    }
}
