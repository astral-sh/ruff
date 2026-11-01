use ruff_python_ast::{Expr, Int};

use ruff_python_semantic::SemanticModel;

pub(super) fn is_sys(expr: Expr<'_>, target: &str, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| qualified_name.segments() == ["sys", target])
}

pub(super) fn integer_literal(expr: Expr<'_>) -> Option<&Int> {
    expr.as_number_literal_expr()?.value().as_int()
}
