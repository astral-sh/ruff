use rustc_hash::FxHashSet;

use crate::analyze::typing;
use crate::{BindingId, SemanticModel};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{Expr, ExprName, ExprStarred, ExprSubscript, ExprTuple};

/// Return `true` if any base class matches a [`QualifiedName`] predicate.
pub fn any_qualified_base_class(
    class_def: &ast::StmtClassDef,
    semantic: &SemanticModel,
    func: &dyn Fn(QualifiedName) -> bool,
) -> bool {
    any_base_class(class_def, semantic, &mut |expr| {
        semantic
            .resolve_qualified_name(map_subscript(expr))
            .is_some_and(func)
    })
}

/// Return `true` if any base class matches an [`Expr`] predicate.
pub fn any_base_class(
    class_def: &ast::StmtClassDef,
    semantic: &SemanticModel,
    func: &mut dyn FnMut(&Expr) -> bool,
) -> bool {
    fn inner(
        class_def: &ast::StmtClassDef,
        semantic: &SemanticModel,
        func: &mut dyn FnMut(&Expr) -> bool,
        seen: &mut FxHashSet<BindingId>,
    ) -> bool {
        class_def.bases().iter().any(|expr| {
            // If the base class itself matches the pattern, then this does too.
            // Ex) `class Foo(BaseModel): ...`
            if func(expr) {
                return true;
            }

            // If the base class extends a class that matches the pattern, then this does too.
            // Ex) `class Bar(BaseModel): ...; class Foo(Bar): ...`
            if let Some(id) = semantic.lookup_attribute(map_subscript(expr)) {
                if seen.insert(id) {
                    let binding = semantic.binding(id);
                    if let Some(base_class) = binding
                        .kind
                        .as_class_definition()
                        .map(|id| &semantic.scopes[*id])
                        .and_then(|scope| scope.kind.as_class())
                    {
                        if inner(base_class, semantic, func, seen) {
                            return true;
                        }
                    }
                }
            }
            false
        })
    }

    if class_def.bases().is_empty() {
        return false;
    }

    inner(class_def, semantic, func, &mut FxHashSet::default())
}

/// Return `true` if any base class matches an [`ast::StmtClassDef`] predicate.
pub fn any_super_class(
    class_def: &ast::StmtClassDef,
    semantic: &SemanticModel,
    func: &dyn Fn(&ast::StmtClassDef) -> bool,
) -> bool {
    fn inner(
        class_def: &ast::StmtClassDef,
        semantic: &SemanticModel,
        func: &dyn Fn(&ast::StmtClassDef) -> bool,
        seen: &mut FxHashSet<BindingId>,
    ) -> bool {
        // If the function itself matches the pattern, then this does too.
        if func(class_def) {
            return true;
        }

        // Otherwise, check every base class.
        class_def.bases().iter().any(|expr| {
            // If the base class extends a class that matches the pattern, then this does too.
            if let Some(id) = semantic.lookup_attribute(map_subscript(expr)) {
                if seen.insert(id) {
                    let binding = semantic.binding(id);
                    if let Some(base_class) = binding
                        .kind
                        .as_class_definition()
                        .map(|id| &semantic.scopes[*id])
                        .and_then(|scope| scope.kind.as_class())
                    {
                        if inner(base_class, semantic, func, seen) {
                            return true;
                        }
                    }
                }
            }
            false
        })
    }

    inner(class_def, semantic, func, &mut FxHashSet::default())
}

/// Return `true` if `class_def` is a class that has one or more enum classes in its mro
pub fn is_enumeration(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    any_qualified_base_class(class_def, semantic, &|qualified_name| {
        matches!(
            qualified_name.segments(),
            [
                "enum",
                "Enum" | "Flag" | "IntEnum" | "IntFlag" | "StrEnum" | "ReprEnum" | "CheckEnum"
            ]
        )
    })
}

/// Whether or not a class is a metaclass. Constructed by [`is_metaclass`].
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum IsMetaclass {
    Yes,
    No,
    Maybe,
}

impl From<IsMetaclass> for bool {
    fn from(value: IsMetaclass) -> Self {
        matches!(value, IsMetaclass::Yes)
    }
}

/// Returns `IsMetaclass::Yes` if the given class is definitely a metaclass,
/// `IsMetaclass::No` if it's definitely *not* a metaclass, and
/// `IsMetaclass::Maybe` otherwise.
pub fn is_metaclass(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> IsMetaclass {
    let mut maybe = false;
    let is_base_class = any_base_class(class_def, semantic, &mut |expr| match expr {
        Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) => {
            maybe = true;
            // Ex) `class Foo(type(Protocol)): ...`
            arguments.len() == 1 && semantic.match_builtin_expr(func.as_ref(), "type")
        }
        Expr::Subscript(ast::ExprSubscript { value, .. }) => {
            // Ex) `class Foo(type[int]): ...`
            semantic.match_builtin_expr(value.as_ref(), "type")
        }
        _ => semantic
            .resolve_qualified_name(expr)
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
                    ["" | "builtins", "type"]
                        | ["abc", "ABCMeta"]
                        | ["enum", "EnumMeta" | "EnumType"]
                )
            }),
    });

    match (is_base_class, maybe) {
        (true, true) => IsMetaclass::Maybe,
        (true, false) => IsMetaclass::Yes,
        (false, _) => IsMetaclass::No,
    }
}

/// Returns true if a class is generic.
///
/// A class is generic if at least one of its direct bases is subscripted with a `TypeVar`-like,
/// or if it is defined using PEP 695 syntax.
///
/// This should only be used in stub context to avoid false positives and negatives.
pub fn is_generic(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    if class_def.type_params.is_some() {
        return true;
    }

    class_def.bases().iter().any(|base| {
        let Expr::Subscript(ExprSubscript { slice, .. }) = base else {
            return false;
        };

        match slice.as_ref() {
            Expr::Name(name) => is_old_style_typevar_like(name, semantic),

            Expr::Tuple(ExprTuple { elts, .. }) => elts.iter().any(|elt| match elt {
                Expr::Name(name) => is_old_style_typevar_like(name, semantic),

                Expr::Starred(ExprStarred { value, .. }) => match value.as_ref() {
                    Expr::Name(name) => is_old_style_typevar_like(name, semantic),
                    _ => false,
                },

                _ => false,
            }),

            Expr::Starred(ExprStarred { value, .. }) => match value.as_ref() {
                Expr::Name(name) => is_old_style_typevar_like(name, semantic),
                _ => false,
            },

            _ => false,
        }
    })
}

fn is_old_style_typevar_like(name: &ExprName, semantic: &SemanticModel) -> bool {
    let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
        return false;
    };

    typing::is_type_var_like(binding, semantic)
}
