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

/// Returns `true` if the given [`Expr`] is a stdlib `dataclasses.field` call.
fn is_stdlib_dataclass_field(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["dataclasses", "field"]))
}

/// Returns `true` if the given [`Expr`] is a call to `attr.ib()` or `attrs.field()`.
fn is_attrs_field(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["attrs", "field" | "Factory"] | ["attr", "ib"]
            )
        })
}

/// Return `true` if `func` represents a `field()` call corresponding to the `dataclass_kind` variant passed in.
///
/// I.e., if `DataclassKind::Attrs` is passed in,
/// return `true` if `func` represents a call to `attr.ib()` or `attrs.field()`;
/// if `DataclassKind::Stdlib` is passed in,
/// return `true` if `func` represents a call to `dataclasse.field()`.
pub(super) fn is_dataclass_field(
    func: &Expr,
    semantic: &SemanticModel,
    dataclass_kind: DataclassKind,
) -> bool {
    match dataclass_kind {
        DataclassKind::Attrs => is_attrs_field(func, semantic),
        DataclassKind::Stdlib => is_stdlib_dataclass_field(func, semantic),
    }
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

/// Enumeration of various kinds of dataclasses recognised by Ruff
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum DataclassKind {
    /// dataclasses created by the stdlib `dataclasses` module
    Stdlib,
    /// dataclasses created by the third-party `attrs` library
    Attrs,
}

impl DataclassKind {
    pub(super) const fn is_stdlib(self) -> bool {
        matches!(self, DataclassKind::Stdlib)
    }

    pub(super) const fn is_attrs(self) -> bool {
        matches!(self, DataclassKind::Attrs)
    }
}

/// Return the kind of dataclass this class definition is (stdlib or `attrs`), or `None` if the class is not a dataclass.
pub(super) fn dataclass_kind(
    class_def: &ast::StmtClassDef,
    semantic: &SemanticModel,
) -> Option<DataclassKind> {
    if !(semantic.seen_module(Modules::DATACLASSES) || semantic.seen_module(Modules::ATTRS)) {
        return None;
    }

    for decorator in &class_def.decorator_list {
        let Some(qualified_name) =
            semantic.resolve_qualified_name(map_callable(&decorator.expression))
        else {
            continue;
        };

        match qualified_name.segments() {
            ["attrs", "define" | "frozen"] | ["attr", "s"] => return Some(DataclassKind::Attrs),
            ["dataclasses", "dataclass"] => return Some(DataclassKind::Stdlib),
            _ => continue,
        }
    }

    None
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
