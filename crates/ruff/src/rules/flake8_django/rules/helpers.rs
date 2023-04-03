use rustpython_parser::ast::Expr;

use ruff_python_semantic::context::Context;

/// Return `true` if a Python class appears to be a Django model, based on its base classes.
pub fn is_model(context: &Context, base: &Expr) -> bool {
    context.resolve_call_path(base).map_or(false, |call_path| {
        call_path.as_slice() == ["django", "db", "models", "Model"]
    })
}

/// Return `true` if a Python class appears to be a Django model form, based on its base classes.
pub fn is_model_form(context: &Context, base: &Expr) -> bool {
    context.resolve_call_path(base).map_or(false, |call_path| {
        call_path.as_slice() == ["django", "forms", "ModelForm"]
            || call_path.as_slice() == ["django", "forms", "models", "ModelForm"]
    })
}

/// Return `true` if the expression is constructor for a Django model field.
pub fn is_model_field(context: &Context, expr: &Expr) -> bool {
    context.resolve_call_path(expr).map_or(false, |call_path| {
        call_path
            .as_slice()
            .starts_with(&["django", "db", "models"])
    })
}

/// Return the name of the field type, if the expression is constructor for a Django model field.
pub fn get_model_field_name<'a>(context: &'a Context, expr: &'a Expr) -> Option<&'a str> {
    context.resolve_call_path(expr).and_then(|call_path| {
        let call_path = call_path.as_slice();
        if !call_path.starts_with(&["django", "db", "models"]) {
            return None;
        }
        call_path.last().copied()
    })
}
