use rustpython_ast::{Expr, ExprKind};

pub fn match_name_or_attr(expr: &Expr, target: &str) -> bool {
    match &expr.node {
        ExprKind::Attribute { attr, .. } => target == attr,
        ExprKind::Name { id, .. } => target == id,
        _ => false,
    }
}
