use ruff_python_ast::Expr;

use ruff_python_semantic::SemanticModel;

pub(super) fn is_sys(expr: &Expr, target: &str, semantic: &SemanticModel) -> bool {
    semantic.matches_qualified_name(expr, &["sys", target])
}
