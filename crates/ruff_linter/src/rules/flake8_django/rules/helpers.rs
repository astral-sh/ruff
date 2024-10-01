use ruff_python_ast::{self as ast, Expr};

use ruff_python_semantic::{analyze, SemanticModel};

/// Return `true` if a Python class appears to be a Django model, based on its base classes.
pub(super) fn is_model(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    analyze::class::any_qualified_base_class(class_def, semantic, &|qualified_name| {
        matches!(
            qualified_name.segments(),
            ["django", "db", "models", "Model"]
        )
    })
}

/// Return `true` if a Python class appears to be a Django model form, based on its base classes.
pub(super) fn is_model_form(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    analyze::class::any_qualified_base_class(class_def, semantic, &|qualified_name| {
        matches!(
            qualified_name.segments(),
            ["django", "forms", "ModelForm"] | ["django", "forms", "models", "ModelForm"]
        )
    })
}

/// Return `true` if the expression is constructor for a Django model field.
pub(super) fn is_model_field(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            qualified_name
                .segments()
                .starts_with(&["django", "db", "models"])
        })
}

/// Return the name of the field type, if the expression is constructor for a Django model field.
pub(super) fn get_model_field_name<'a>(
    expr: &'a Expr,
    semantic: &'a SemanticModel,
) -> Option<&'a str> {
    semantic
        .resolve_qualified_name(expr)
        .and_then(|qualified_name| {
            let qualified_name = qualified_name.segments();
            if !qualified_name.starts_with(&["django", "db", "models"]) {
                return None;
            }
            qualified_name.last().copied()
        })
}
