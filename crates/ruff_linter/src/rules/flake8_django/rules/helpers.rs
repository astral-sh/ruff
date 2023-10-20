use ruff_python_ast::Expr;

use ruff_python_semantic::SemanticModel;

/// Return `true` if a Python class appears to be a Django model, based on its base classes.
pub(super) fn is_model(base: &Expr, semantic: &SemanticModel) -> bool {
    semantic.resolve_call_path(base).is_some_and(|call_path| {
        matches!(call_path.as_slice(), ["django", "db", "models", "Model"])
    })
}

/// Return `true` if a Python class appears to be a Django model form, based on its base classes.
pub(super) fn is_model_form(base: &Expr, semantic: &SemanticModel) -> bool {
    semantic.resolve_call_path(base).is_some_and(|call_path| {
        matches!(
            call_path.as_slice(),
            ["django", "forms", "ModelForm"] | ["django", "forms", "models", "ModelForm"]
        )
    })
}

/// Return `true` if the expression is constructor for a Django model field.
pub(super) fn is_model_field(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic.resolve_call_path(expr).is_some_and(|call_path| {
        call_path
            .as_slice()
            .starts_with(&["django", "db", "models"])
    })
}

/// Return the name of the field type, if the expression is constructor for a Django model field.
pub(super) fn get_model_field_name<'a>(
    expr: &'a Expr,
    semantic: &'a SemanticModel,
) -> Option<&'a str> {
    semantic.resolve_call_path(expr).and_then(|call_path| {
        let call_path = call_path.as_slice();
        if !call_path.starts_with(&["django", "db", "models"]) {
            return None;
        }
        call_path.last().copied()
    })
}
