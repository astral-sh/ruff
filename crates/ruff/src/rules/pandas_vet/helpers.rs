use rustpython_parser::ast::{Expr, ExprKind};

/// Return `true` if an `Expr` _could_ be a `DataFrame`. This rules out
/// obviously-wrong cases, like constants and literals.
pub const fn is_dataframe_candidate(expr: &Expr) -> bool {
    !matches!(
        expr.node,
        ExprKind::Constant { .. }
            | ExprKind::Tuple { .. }
            | ExprKind::List { .. }
            | ExprKind::Set { .. }
            | ExprKind::Dict { .. }
            | ExprKind::SetComp { .. }
            | ExprKind::ListComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::GeneratorExp { .. }
    )
}
