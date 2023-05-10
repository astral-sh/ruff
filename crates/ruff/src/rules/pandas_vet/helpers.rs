use rustpython_parser::ast::{Expr, ExprKind};

/// Return `true` if an `Expr` _could_ be a `DataFrame`. This rules out
/// obviously-wrong cases, like constants and literals.
pub const fn is_dataframe_candidate(expr: &Expr) -> bool {
    !matches!(
        expr.node,
        ExprKind::Constant(_)
            | ExprKind::Tuple(_)
            | ExprKind::List(_)
            | ExprKind::Set(_)
            | ExprKind::Dict(_)
            | ExprKind::SetComp(_)
            | ExprKind::ListComp(_)
            | ExprKind::DictComp(_)
            | ExprKind::GeneratorExp(_)
    )
}
