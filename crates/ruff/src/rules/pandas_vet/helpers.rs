use rustpython_parser::ast::Expr;

/// Return `true` if an `Expr` _could_ be a `DataFrame`. This rules out
/// obviously-wrong cases, like constants and literals.
pub(crate) const fn is_dataframe_candidate(expr: &Expr) -> bool {
    !matches!(
        expr,
        Expr::Constant(_)
            | Expr::Tuple(_)
            | Expr::List(_)
            | Expr::Set(_)
            | Expr::Dict(_)
            | Expr::SetComp(_)
            | Expr::ListComp(_)
            | Expr::DictComp(_)
            | Expr::GeneratorExp(_)
    )
}
