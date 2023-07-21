use rustpython_parser::ast::Expr;

use ruff_python_semantic::SemanticModel;

pub(super) fn is_sys(expr: &Expr, target: &str, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(expr)
        .map_or(false, |call_path| call_path.as_slice() == ["sys", target])
}
