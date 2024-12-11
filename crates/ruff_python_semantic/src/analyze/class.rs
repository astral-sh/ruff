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

impl IsMetaclass {
    pub const fn is_yes(self) -> bool {
        matches!(self, IsMetaclass::Yes)
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

/// Returns true if a class might be generic.
///
/// A class is considered generic if at least one of its direct bases
/// is subscripted with a `TypeVar`-like,
/// or if it is defined using PEP 695 syntax.
///
/// Therefore, a class *might* be generic if it uses PEP-695 syntax
/// or at least one of its direct bases is a subscript expression that
/// is subscripted with an object that *might* be a `TypeVar`-like.
pub fn might_be_generic(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    if class_def.type_params.is_some() {
        return true;
    }

    class_def.bases().iter().any(|base| {
        let Expr::Subscript(ExprSubscript { slice, .. }) = base else {
            return false;
        };

        let Expr::Tuple(ExprTuple { elts, .. }) = slice.as_ref() else {
            return expr_might_be_typevar_like(slice, semantic);
        };

        elts.iter()
            .any(|elt| expr_might_be_typevar_like(elt, semantic))
    })
}

fn expr_might_be_typevar_like(expr: &Expr, semantic: &SemanticModel) -> bool {
    is_known_typevar(expr, semantic) || expr_might_be_old_style_typevar_like(expr, semantic)
}

fn is_known_typevar(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic.match_typing_expr(expr, "AnyStr")
}

fn expr_might_be_old_style_typevar_like(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        Expr::Attribute(..) => true,
        Expr::Name(name) => might_be_old_style_typevar_like(name, semantic),
        Expr::Starred(ExprStarred { value, .. }) => {
            expr_might_be_old_style_typevar_like(value, semantic)
        }
        _ => false,
    }
}

fn might_be_old_style_typevar_like(name: &ExprName, semantic: &SemanticModel) -> bool {
    let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
        return !semantic.has_builtin_binding(&name.id);
    };
    typing::is_type_var_like(binding, semantic)
}
