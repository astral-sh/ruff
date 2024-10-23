use ruff_python_ast::helpers::{map_callable, map_subscript};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::{analyze, BindingKind, Modules, SemanticModel};

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
    if !semantic.seen_module(Modules::DATACLASSES) {
        return false;
    }

    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["dataclasses", "field"]))
}

/// Returns `true` if the given [`Expr`] is a `typing.ClassVar` annotation.
pub(super) fn is_class_var_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
    if !semantic.seen_typing() {
        return false;
    }

    // ClassVar can be used either with a subscript `ClassVar[...]` or without (the type is
    // inferred).
    semantic.match_typing_expr(map_subscript(annotation), "ClassVar")
}

/// Returns `true` if the given [`Expr`] is a `typing.Final` annotation.
pub(super) fn is_final_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
    if !semantic.seen_typing() {
        return false;
    }

    // Final can be used either with a subscript `Final[...]` or without (the type is
    // inferred).
    semantic.match_typing_expr(map_subscript(annotation), "Final")
}

/// Returns `true` if the given class is a dataclass.
pub(super) fn is_dataclass(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    if !semantic.seen_module(Modules::DATACLASSES) {
        return false;
    }

    class_def.decorator_list.iter().any(|decorator| {
        semantic
            .resolve_qualified_name(map_callable(&decorator.expression))
            .is_some_and(|qualified_name| {
                matches!(qualified_name.segments(), ["dataclasses", "dataclass"])
            })
    })
}

/// Returns `true` if the given class has "default copy" semantics.
///
/// For example, Pydantic `BaseModel` and `BaseSettings` subclassses copy attribute defaults on
/// instance creation. As such, the use of mutable default values is safe for such classes.
pub(super) fn has_default_copy_semantics(
    class_def: &ast::StmtClassDef,
    semantic: &SemanticModel,
) -> bool {
    analyze::class::any_qualified_base_class(class_def, semantic, &|qualified_name| {
        matches!(
            qualified_name.segments(),
            ["pydantic", "BaseModel" | "BaseSettings" | "BaseConfig"]
                | ["pydantic_settings", "BaseSettings"]
                | ["msgspec", "Struct"]
        )
    })
}

/// Returns `true` if the given function is an instantiation of a class that implements the
/// descriptor protocol.
///
/// See: <https://docs.python.org/3.10/reference/datamodel.html#descriptors>
pub(super) fn is_descriptor_class(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic.lookup_attribute(func).is_some_and(|id| {
        let BindingKind::ClassDefinition(scope_id) = semantic.binding(id).kind else {
            return false;
        };

        // Look for `__get__`, `__set__`, and `__delete__` methods.
        ["__get__", "__set__", "__delete__"].iter().any(|method| {
            semantic.scopes[scope_id]
                .get(method)
                .is_some_and(|id| semantic.binding(id).kind.is_function_definition())
        })
    })
}
