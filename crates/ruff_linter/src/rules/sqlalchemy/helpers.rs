use ruff_python_ast::Expr;
use ruff_python_semantic::SemanticModel;

pub(in crate::rules::sqlalchemy) fn is_mapped_attribute(
    expr: &Expr,
    semantic: &SemanticModel,
) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            [
                ["sqlalchemy", "ext", "associationproxy"],
                ["sqlalchemy", "orm", "column_property"],
                ["sqlalchemy", "orm", "composite"],
                ["sqlalchemy", "orm", "mapped_column"],
                ["sqlalchemy", "orm", "relationship"],
                ["sqlalchemy", "orm", "synonym"],
            ]
            .iter()
            .any(|n| qualified_name.segments().starts_with(n))
        })
}
