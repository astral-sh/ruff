use ast::Keyword;
use ruff_python_ast::helpers::{map_callable, map_subscript};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_parser::parse_expression;
use ruff_python_semantic::{analyze, BindingKind, SemanticModel};
use rustc_hash::FxHashSet;

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
    semantic
        .resolve_call_path(func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["dataclasses", "field"]))
}

/// Returns `true` if the given [`Expr`] is a `typing.ClassVar` annotation.
pub(super) fn is_class_var_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
    // ClassVar can be used either with a subscript `ClassVar[...]` or without (the type is
    // inferred).
    semantic.match_typing_expr(map_subscript(annotation), "ClassVar")
}

/// Returns `true` if the given [`Expr`] is a `typing.Final` annotation.
pub(super) fn is_final_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
    // Final can be used either with a subscript `Final[...]` or without (the type is
    // inferred).
    semantic.match_typing_expr(map_subscript(annotation), "Final")
}

/// Returns `true` if the given class is a dataclass.
pub(super) fn is_dataclass(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    class_def.decorator_list.iter().any(|decorator| {
        semantic
            .resolve_call_path(map_callable(&decorator.expression))
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["dataclasses", "dataclass"]))
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
    analyze::class::any_call_path(class_def, semantic, &|call_path| {
        matches!(
            call_path.as_slice(),
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

// fast check to disqualify any string literal that doesn't
// have opening and closing brackets
#[inline]
fn has_brackets(possible_fstring: &str) -> bool {
    let mut opening = false;
    let mut closing = false;
    for c in possible_fstring.chars() {
        match c {
            '{' => opening = true,
            '}' => closing = true,
            _ => {}
        }
    }
    opening && closing
}

/// Returns `true` if `source` is valid f-string syntax with qualified, bound variables.
/// `kwargs` should be the keyword arguments that were passed to function if the string literal is also
/// being passed to the same function.
/// If a identifier from `kwargs` is used in `source`'s formatting, this will return `false`,
/// since it's possible the function could be formatting the literal in question.
pub(super) fn should_be_fstring(
    source: &str,
    kwargs: Option<&Vec<Keyword>>,
    semantic: &SemanticModel,
) -> bool {
    if !has_brackets(source) {
        return false;
    }

    let Ok(Expr::FString(ast::ExprFString { value, .. })) =
        parse_expression(&format!("f\"{source}\""))
    else {
        return false;
    };

    let kw_idents: FxHashSet<String> = kwargs
        .map(|keywords| {
            keywords
                .iter()
                .filter_map(|k| k.arg.clone())
                .map(Into::into)
                .collect()
        })
        .unwrap_or_default();

    for f_string in value.f_strings() {
        let mut has_name = false;
        for element in &f_string.elements {
            let Some(ast::FStringExpressionElement { expression, .. }) = element.as_expression()
            else {
                continue;
            };

            if let Expr::Name(ast::ExprName { id, .. }) = expression.as_ref() {
                if kw_idents.contains(id) || semantic.lookup_symbol(id.as_str()).is_none() {
                    return false;
                }
                has_name = true;
            }
        }
        if !has_name {
            return false;
        }
    }

    true
}
