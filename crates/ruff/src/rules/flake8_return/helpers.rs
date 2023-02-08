use rustpython_parser::ast::{Constant, Expr, ExprKind, Stmt};

/// Return `true` if a function's return statement include at least one
/// non-`None` value.
pub fn result_exists(returns: &[(&Stmt, Option<&Expr>)]) -> bool {
    returns.iter().any(|(_, expr)| {
        expr.map(|expr| {
            !matches!(
                expr.node,
                ExprKind::Constant {
                    value: Constant::None,
                    ..
                }
            )
        })
        .unwrap_or(false)
    })
}
