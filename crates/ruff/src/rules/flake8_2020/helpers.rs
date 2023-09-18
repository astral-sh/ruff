use ruff_python_ast::Expr;

use ruff_python_semantic::SemanticModel;

pub(super) fn is_sys(expr: &Expr, target: &str, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(expr)
        .is_some_and(|call_path| call_path.as_slice() == ["sys", target])
}
