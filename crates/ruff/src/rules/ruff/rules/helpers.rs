use rustpython_parser::ast::{self, Expr};

use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::SemanticModel;

/// Return `true` if the given [`Expr`] is a special class attribute, like `__slots__`.
///
/// While `__slots__` is typically defined via a tuple, Python accepts any iterable and, in
/// particular, allows the use of a dictionary to define the attribute names (as keys) and
/// docstrings (as values).
pub(super) fn is_special_attribute(value: &Expr) -> bool {
    if let Expr::Name(ast::ExprName { id, .. }) = value {
        matches!(
            id.as_str(),
            "__slots__" | "__dict__" | "__weakref__" | "__annotations__"
        )
    } else {
        false
    }
}

/// Returns `true` if the given [`Expr`] is a `dataclasses.field` call.
pub(super) fn is_dataclass_field(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic.resolve_call_path(func).map_or(false, |call_path| {
        matches!(call_path.as_slice(), ["dataclasses", "field"])
    })
}

/// Returns `true` if the given [`Expr`] is a `typing.ClassVar` annotation.
pub(super) fn is_class_var_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Subscript(ast::ExprSubscript { value, .. }) = &annotation else {
        return false;
    };
    semantic.match_typing_expr(value, "ClassVar")
}

/// Returns `true` if the given [`Expr`] is a `typing.Final` annotation.
pub(super) fn is_final_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Subscript(ast::ExprSubscript { value, .. }) = &annotation else {
        return false;
    };
    semantic.match_typing_expr(value, "Final")
}

/// Returns `true` if the given class is a dataclass.
pub(super) fn is_dataclass(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    class_def.decorator_list.iter().any(|decorator| {
        semantic
            .resolve_call_path(map_callable(&decorator.expression))
            .map_or(false, |call_path| {
                matches!(call_path.as_slice(), ["dataclasses", "dataclass"])
            })
    })
}

/// Returns `true` if the given class is a Pydantic `BaseModel` or `BaseSettings` subclass.
pub(super) fn is_pydantic_model(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    class_def.bases.iter().any(|expr| {
        semantic.resolve_call_path(expr).map_or(false, |call_path| {
            matches!(
                call_path.as_slice(),
                ["pydantic", "BaseModel" | "BaseSettings"]
            )
        })
    })
}
