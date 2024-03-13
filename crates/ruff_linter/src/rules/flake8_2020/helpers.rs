use ruff_python_ast::Expr;

use ruff_python_semantic::SemanticModel;

pub(super) fn is_sys(expr: &Expr, target: &str, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| qualified_name.segments() == ["sys", target])
}
