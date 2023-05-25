use ruff_python_semantic::{
    model::SemanticModel,
    scope::{FunctionDef, ScopeKind},
};

/// Return `true` if the [`SemanticModel`] is inside an async function definition.
pub(crate) fn in_async_function(model: &SemanticModel) -> bool {
    model
        .scopes()
        .find_map(|scope| {
            if let ScopeKind::Function(FunctionDef { async_, .. }) = &scope.kind {
                Some(*async_)
            } else {
                None
            }
        })
        .unwrap_or(false)
}
