use ruff_python_ast::helpers::{map_callable, map_subscript, Truthiness};
use ruff_python_ast::{self as ast, Expr, ExprCall};
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
        DataclassKind::Attrs(..) => is_attrs_field(func, semantic),
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

/// Values that [`attrs`'s `auto_attribs`][1] accept.
///
/// [1]: https://www.attrs.org/en/stable/api.html#attrs.define
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum AttrsAutoAttribs {
    /// `a: str = ...` are automatically converted to fields.
    True,
    /// Only `attrs.field()`/`attr.ib()` calls are considered fields.
    False,
    /// `True` if any attributes are annotated (and no unannotated `attrs.field`s are found).
    /// `False` otherwise.
    None,
    /// The provided value is not a literal.
    Unknown,
}

/// Enumeration of various kinds of dataclasses recognised by Ruff
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum DataclassKind {
    /// dataclasses created by the stdlib `dataclasses` module
    Stdlib,
    /// dataclasses created by the third-party `attrs` library
    Attrs(AttrsAutoAttribs),
}

/// Return the kind of dataclass this class definition is (stdlib or `attrs`),
/// or `None` if the class is not a dataclass.
pub(super) fn dataclass_kind<'a>(
    class_def: &'a ast::StmtClassDef,
    semantic: &SemanticModel,
) -> Option<(DataclassKind, &'a ast::Decorator)> {
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
            ["attrs", func @ ("define" | "frozen" | "mutable")] | ["attr", func @ "s"] => {
                // `.define`, `.frozen` and `.mutable` all default `auto_attribs` to `None`,
                // whereas `@attr.s` implicitly sets `auto_attribs=False`.
                // https://www.attrs.org/en/stable/api.html#attrs.define
                // https://www.attrs.org/en/stable/api-attr.html#attr.s
                let Expr::Call(ExprCall { arguments, .. }) = &decorator.expression else {
                    let auto_attribs = if *func == "s" {
                        AttrsAutoAttribs::False
                    } else {
                        AttrsAutoAttribs::None
                    };

                    return Some((DataclassKind::Attrs(auto_attribs), decorator));
                };

                let Some(auto_attribs) = arguments.find_keyword("auto_attribs") else {
                    return Some((DataclassKind::Attrs(AttrsAutoAttribs::None), decorator));
                };

                let auto_attribs = match Truthiness::from_expr(&auto_attribs.value, |id| {
                    semantic.has_builtin_binding(id)
                }) {
                    // `auto_attribs` requires an exact `True` to be true
                    Truthiness::True => AttrsAutoAttribs::True,
                    // Or an exact `None` to auto-detect.
                    Truthiness::None => AttrsAutoAttribs::None,
                    // Otherwise, anything else (even a truthy value, like `1`) is considered `False`.
                    Truthiness::Truthy | Truthiness::False | Truthiness::Falsey => {
                        AttrsAutoAttribs::False
                    }
                    // Unless, of course, we can't determine the value.
                    Truthiness::Unknown => AttrsAutoAttribs::Unknown,
                };

                return Some((DataclassKind::Attrs(auto_attribs), decorator));
            }
            ["dataclasses", "dataclass"] => return Some((DataclassKind::Stdlib, decorator)),
            _ => continue,
        }
    }

    None
}

/// Returns `true` if the given class has "default copy" semantics.
///
/// For example, Pydantic `BaseModel` and `BaseSettings` subclasses copy attribute defaults on
/// instance creation. As such, the use of mutable default values is safe for such classes.
pub(super) fn has_default_copy_semantics(
    class_def: &ast::StmtClassDef,
    semantic: &SemanticModel,
) -> bool {
    analyze::class::any_qualified_base_class(class_def, semantic, &|qualified_name| {
        matches!(
            qualified_name.segments(),
            [
                "pydantic",
                "BaseModel" | "RootModel" | "BaseSettings" | "BaseConfig"
            ] | ["pydantic", "generics", "GenericModel"]
                | [
                    "pydantic",
                    "v1",
                    "BaseModel" | "BaseSettings" | "BaseConfig"
                ]
                | ["pydantic", "v1", "generics", "GenericModel"]
                | ["pydantic_settings", "BaseSettings"]
                | ["msgspec", "Struct"]
                | ["sqlmodel", "SQLModel"]
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
