use std::collections::VecDeque;

use rustc_hash::FxHashSet;

use crate::analyze::typing;
use crate::{BindingId, SemanticModel};
use ruff_python_ast::flat::helpers::map_subscript;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{
    Expr, ExprName, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFunctionDef,
    flat::{self, fields as ast},
};

/// Return `true` if any base class matches a [`QualifiedName`] predicate.
pub fn any_qualified_base_class<F>(
    class_def: flat::View<'_, StmtClassDef>,
    semantic: &SemanticModel,
    mut func: F,
) -> bool
where
    F: FnMut(QualifiedName) -> bool,
{
    any_base_class(class_def, semantic, |expr| {
        semantic
            .resolve_qualified_name(map_subscript(expr))
            .is_some_and(&mut func)
    })
}

/// Return `true` if any base class matches an [`Expr`] predicate.
pub fn any_base_class<F>(
    class_def: flat::View<'_, StmtClassDef>,
    semantic: &SemanticModel,
    mut func: F,
) -> bool
where
    F: FnMut(flat::View<'_, Expr>) -> bool,
{
    fn inner(
        class_def: flat::View<'_, StmtClassDef>,
        semantic: &SemanticModel,
        func: &mut dyn FnMut(flat::View<'_, Expr>) -> bool,
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
                        if inner(*base_class, semantic, func, seen) {
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

    inner(class_def, semantic, &mut func, &mut FxHashSet::default())
}

/// Returns an iterator over all base classes, beginning with the
/// given class.
///
/// The traversal of the class hierarchy is breadth-first, since
/// this graph tends to have small width but could be rather deep.
pub fn iter_super_class<'stmt>(
    class_def: flat::View<'stmt, StmtClassDef>,
    semantic: &'stmt SemanticModel,
) -> impl Iterator<Item = flat::View<'stmt, StmtClassDef>> {
    struct SuperClassIterator<'stmt> {
        semantic: &'stmt SemanticModel<'stmt>,
        to_visit: VecDeque<flat::View<'stmt, StmtClassDef>>,
        seen: FxHashSet<BindingId>,
    }

    impl<'a> Iterator for SuperClassIterator<'a> {
        type Item = flat::View<'a, StmtClassDef>;

        fn next(&mut self) -> Option<Self::Item> {
            let current = self.to_visit.pop_front()?;

            // Add all base classes to the to_visit list
            for expr in current.bases() {
                if let Some(id) = self.semantic.lookup_attribute(map_subscript(expr)) {
                    if self.seen.insert(id) {
                        let binding = self.semantic.binding(id);
                        if let Some(base_class) = binding
                            .kind
                            .as_class_definition()
                            .map(|id| &self.semantic.scopes[*id])
                            .and_then(|scope| scope.kind.as_class())
                        {
                            self.to_visit.push_back(*base_class);
                        }
                    }
                }
            }
            Some(current)
        }
    }

    SuperClassIterator {
        semantic,
        to_visit: VecDeque::from([class_def]),
        seen: FxHashSet::default(),
    }
}

/// Return `true` if any base class, including the given class,
/// matches an [`StmtClassDef`] predicate.
pub fn any_super_class<F>(
    class_def: flat::View<'_, StmtClassDef>,
    semantic: &SemanticModel,
    func: F,
) -> bool
where
    F: Fn(flat::View<'_, StmtClassDef>) -> bool,
{
    iter_super_class(class_def, semantic).any(func)
}

#[derive(Clone, Debug)]
pub struct ClassMemberDeclaration<'a> {
    kind: ClassMemberKind<'a>,
    boundness: ClassMemberBoundness,
}

impl<'a> ClassMemberDeclaration<'a> {
    pub fn kind(&self) -> &ClassMemberKind<'a> {
        &self.kind
    }

    pub fn boundness(&self) -> ClassMemberBoundness {
        self.boundness
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ClassMemberBoundness {
    PossiblyUnbound,
    Bound,
}

#[derive(Copy, Clone, Debug)]
pub enum ClassMemberKind<'a> {
    Assign(flat::View<'a, StmtAssign>),
    AnnAssign(flat::View<'a, StmtAnnAssign>),
    FunctionDef(flat::View<'a, StmtFunctionDef>),
}

pub fn any_member_declaration<F>(class: flat::View<'_, StmtClassDef>, mut func: F) -> bool
where
    F: FnMut(ClassMemberDeclaration) -> bool,
{
    fn any_stmt_in_body(
        body: flat::Suite<'_>,
        func: &mut dyn FnMut(ClassMemberDeclaration) -> bool,
        boundness: ClassMemberBoundness,
    ) -> bool {
        body.iter().any(|stmt| {
            let kind = match stmt.fields() {
                ast::Stmt::FunctionDef(ast::StmtFunctionDef {
                    node: function_def, ..
                }) => Some(ClassMemberKind::FunctionDef(function_def)),
                ast::Stmt::Assign(ast::StmtAssign { node: assign, .. }) => {
                    Some(ClassMemberKind::Assign(assign))
                }
                ast::Stmt::AnnAssign(ast::StmtAnnAssign { node: assign, .. }) => {
                    Some(ClassMemberKind::AnnAssign(assign))
                }
                ast::Stmt::With(ast::StmtWith { body, .. }) => {
                    if any_stmt_in_body(body, func, ClassMemberBoundness::PossiblyUnbound) {
                        return true;
                    }

                    None
                }

                ast::Stmt::For(ast::StmtFor { body, orelse, .. })
                | ast::Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
                    if any_stmt_in_body(body, func, ClassMemberBoundness::PossiblyUnbound)
                        || any_stmt_in_body(orelse, func, ClassMemberBoundness::PossiblyUnbound)
                    {
                        return true;
                    }

                    None
                }

                ast::Stmt::If(ast::StmtIf {
                    body,
                    elif_else_clauses,
                    ..
                }) => {
                    if any_stmt_in_body(body, func, ClassMemberBoundness::PossiblyUnbound)
                        || elif_else_clauses.iter().any(|it| {
                            any_stmt_in_body(it.body(), func, ClassMemberBoundness::PossiblyUnbound)
                        })
                    {
                        return true;
                    }
                    None
                }

                ast::Stmt::Match(ast::StmtMatch { cases, .. }) => {
                    if cases.iter().any(|it| {
                        any_stmt_in_body(it.body(), func, ClassMemberBoundness::PossiblyUnbound)
                    }) {
                        return true;
                    }

                    None
                }

                ast::Stmt::Try(ast::StmtTry {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                    ..
                }) => {
                    if any_stmt_in_body(body, func, ClassMemberBoundness::PossiblyUnbound)
                        || any_stmt_in_body(orelse, func, ClassMemberBoundness::PossiblyUnbound)
                        || any_stmt_in_body(finalbody, func, ClassMemberBoundness::PossiblyUnbound)
                        || handlers
                            .iter()
                            .filter_map(flat::View::as_except_handler)
                            .any(|it| {
                                any_stmt_in_body(
                                    it.body(),
                                    func,
                                    ClassMemberBoundness::PossiblyUnbound,
                                )
                            })
                    {
                        return true;
                    }

                    None
                }
                // Technically, a method can be defined using a few more methods:
                //
                // ```python
                // class C1:
                //     # Import
                //     import __eq__  # Callable module
                //     # ImportFrom
                //     from module import __eq__  # Top level callable
                //     # ExprNamed
                //     (__eq__ := lambda self, other: True)
                // ```
                //
                // Those cases are not yet supported because they're rare.
                ast::Stmt::ClassDef(_)
                | ast::Stmt::Return(_)
                | ast::Stmt::Delete(_)
                | ast::Stmt::AugAssign(_)
                | ast::Stmt::TypeAlias(_)
                | ast::Stmt::Raise(_)
                | ast::Stmt::Assert(_)
                | ast::Stmt::Import(_)
                | ast::Stmt::ImportFrom(_)
                | ast::Stmt::Global(_)
                | ast::Stmt::Nonlocal(_)
                | ast::Stmt::Expr(_)
                | ast::Stmt::Pass(_)
                | ast::Stmt::Break(_)
                | ast::Stmt::Continue(_)
                | ast::Stmt::IpyEscapeCommand(_) => None,
            };

            if let Some(kind) = kind {
                if func(ClassMemberDeclaration { kind, boundness }) {
                    return true;
                }
            }

            false
        })
    }

    any_stmt_in_body(class.body(), &mut func, ClassMemberBoundness::Bound)
}

/// Return `true` if `class_def` is a class that has one or more enum classes in its mro
pub fn is_enumeration(class_def: flat::View<'_, StmtClassDef>, semantic: &SemanticModel) -> bool {
    any_qualified_base_class(class_def, semantic, |qualified_name| {
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

/// Check if a class has a metaclass-like `__new__` method signature.
///
/// A metaclass-like `__new__` method signature has:
/// 1. Exactly 5 parameters (including cls)
/// 2. Second parameter annotated with `str`
/// 3. Third parameter annotated with a `tuple` type
/// 4. Fourth parameter annotated with a `dict` type
/// 5. Fifth parameter is keyword-variadic (`**kwargs`)
///
/// For example:
///
/// ```python
/// class MyMetaclass(django.db.models.base.ModelBase):
///     def __new__(cls, name: str, bases: tuple[Any, ...], attrs: dict[str, Any], **kwargs: Any) -> MyMetaclass:
///         ...
/// ```
fn has_metaclass_new_signature(
    class_def: flat::View<'_, StmtClassDef>,
    semantic: &SemanticModel,
) -> bool {
    // Look for a __new__ method in the class body
    for stmt in class_def.body() {
        let Some(function_def_node) = stmt.as_function_def_stmt() else {
            continue;
        };
        let name = function_def_node.name();
        let parameters = function_def_node.parameters();

        if name != "__new__" {
            continue;
        }

        // Check if we have exactly 5 parameters (cls + 4 others)
        if parameters.len() != 5 {
            continue;
        }

        // Check that there is no variadic parameter
        if parameters.vararg().is_some() {
            continue;
        }

        // Check that the last parameter is keyword-variadic (**kwargs)
        if parameters.kwarg().is_none() {
            continue;
        }

        // Check parameter annotations, skipping the first parameter (cls)
        let mut param_iter = parameters.iter().skip(1);

        // Check second parameter (name: str)
        let Some(second_param) = param_iter.next() else {
            continue;
        };
        if !second_param
            .annotation()
            .is_some_and(|annotation| semantic.match_builtin_expr(map_subscript(annotation), "str"))
        {
            continue;
        }

        // Check third parameter (bases: tuple[...])
        let Some(third_param) = param_iter.next() else {
            continue;
        };
        if !third_param.annotation().is_some_and(|annotation| {
            semantic.match_builtin_expr(map_subscript(annotation), "tuple")
        }) {
            continue;
        }

        // Check fourth parameter (attrs: dict[...])
        let Some(fourth_param) = param_iter.next() else {
            continue;
        };
        if !fourth_param.annotation().is_some_and(|annotation| {
            semantic.match_builtin_expr(map_subscript(annotation), "dict")
        }) {
            continue;
        }

        return true;
    }

    false
}

/// Returns `IsMetaclass::Yes` if the given class is definitely a metaclass,
/// `IsMetaclass::No` if it's definitely *not* a metaclass, and
/// `IsMetaclass::Maybe` otherwise.
pub fn is_metaclass(
    class_def: flat::View<'_, StmtClassDef>,
    semantic: &SemanticModel,
) -> IsMetaclass {
    let mut maybe = false;
    let is_base_class = any_base_class(class_def, semantic, |expr| match expr.classify() {
        flat::ExprKind::Call(expr_call) => {
            let func = expr_call.func();
            let arguments = expr_call.arguments();

            maybe = true;
            // Ex) `class Foo(type(Protocol)): ...`
            arguments.len() == 1 && semantic.match_builtin_expr(func, "type")
        }
        flat::ExprKind::Subscript(expr_subscript) => {
            let value = expr_subscript.value();

            // Ex) `class Foo(type[int]): ...`
            semantic.match_builtin_expr(value, "type")
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
        (false, _) => {
            // If it has >1 base class and a metaclass-like signature for `__new__`,
            // then it might be a metaclass.
            if class_def.bases().is_empty() {
                IsMetaclass::No
            } else if has_metaclass_new_signature(class_def, semantic) {
                IsMetaclass::Maybe
            } else {
                IsMetaclass::No
            }
        }
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
pub fn might_be_generic(class_def: flat::View<'_, StmtClassDef>, semantic: &SemanticModel) -> bool {
    if class_def.type_params().is_some() {
        return true;
    }

    class_def.bases().iter().any(|base| {
        let Some(subscript_node) = base.as_subscript_expr() else {
            return false;
        };
        let slice = subscript_node.slice();

        let Some(tuple_node) = slice.as_tuple_expr() else {
            return expr_might_be_typevar_like(slice, semantic);
        };
        let elts = tuple_node.elts();

        elts.iter()
            .any(|elt| expr_might_be_typevar_like(elt, semantic))
    })
}

fn expr_might_be_typevar_like(expr: flat::View<'_, Expr>, semantic: &SemanticModel) -> bool {
    is_known_typevar(expr, semantic) || expr_might_be_old_style_typevar_like(expr, semantic)
}

fn is_known_typevar(expr: flat::View<'_, Expr>, semantic: &SemanticModel) -> bool {
    semantic.match_typing_expr(expr, "AnyStr")
}

fn expr_might_be_old_style_typevar_like(
    expr: flat::View<'_, Expr>,
    semantic: &SemanticModel,
) -> bool {
    match expr.fields() {
        ast::Expr::Attribute(..) => true,
        ast::Expr::Name(ast::ExprName { node: name, .. }) => {
            might_be_old_style_typevar_like(name, semantic)
        }
        ast::Expr::Starred(ast::ExprStarred { value, .. }) => {
            expr_might_be_old_style_typevar_like(value, semantic)
        }
        _ => false,
    }
}

fn might_be_old_style_typevar_like(
    name: flat::View<'_, ExprName>,
    semantic: &SemanticModel,
) -> bool {
    let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
        return !semantic.has_builtin_binding(name.id());
    };
    typing::is_type_var_like(binding, semantic)
}
