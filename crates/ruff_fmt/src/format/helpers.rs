use crate::cst::{Expr, ExprKind, Unaryop};

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
        | ExprKind::Subscript { .. } => true,
        ExprKind::BinOp { left, right, .. } => {
            matches!(left.node, ExprKind::Constant { .. })
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
        _ => false,
    }
}

/// Return `true` if an [`Expr`] adheres to Black's definition of a non-complex
/// expression, in the context of a slice operation.
pub fn is_simple(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::UnaryOp { op, operand } => {
            if matches!(op, Unaryop::Not) {
                false
            } else {
                is_simple(operand)
            }
        }
        ExprKind::Constant { .. } => true,
        ExprKind::Name { .. } => true,
        _ => false,
    }
}
