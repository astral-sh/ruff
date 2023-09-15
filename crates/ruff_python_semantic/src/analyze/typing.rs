//! Analysis rules for the `typing` module.

use num_traits::identities::Zero;
use ruff_python_ast::{
    self as ast, Constant, Expr, Operator, ParameterWithDefault, Parameters, Stmt,
};

use crate::analyze::type_inference::{PythonType, ResolvedPythonType};
use crate::{Binding, BindingKind};
use ruff_python_ast::call_path::{from_qualified_name, from_unqualified_name, CallPath};
use ruff_python_ast::helpers::{is_const_false, map_subscript};
use ruff_python_stdlib::typing::{
    as_pep_585_generic, has_pep_585_generic, is_immutable_generic_type,
    is_immutable_non_generic_type, is_immutable_return_type, is_literal_member,
    is_mutable_return_type, is_pep_593_generic_member, is_pep_593_generic_type,
    is_standard_library_generic, is_standard_library_generic_member, is_standard_library_literal,
};
use ruff_text_size::Ranged;

use crate::model::SemanticModel;

#[derive(Copy, Clone)]
pub enum Callable {
    Bool,
    Cast,
    NewType,
    TypeVar,
    NamedTuple,
    TypedDict,
    MypyExtension,
}

#[derive(Copy, Clone)]
pub enum SubscriptKind {
    /// A subscript of the form `typing.Literal["foo", "bar"]`, i.e., a literal.
    Literal,
    /// A subscript of the form `typing.List[int]`, i.e., a generic.
    Generic,
    /// A subscript of the form `typing.Annotated[int, "foo"]`, i.e., a PEP 593 annotation.
    PEP593Annotation,
}

pub fn match_annotated_subscript<'a>(
    expr: &Expr,
    semantic: &SemanticModel,
    typing_modules: impl Iterator<Item = &'a str>,
    extend_generics: &[String],
) -> Option<SubscriptKind> {
    semantic.resolve_call_path(expr).and_then(|call_path| {
        if is_standard_library_literal(call_path.as_slice()) {
            return Some(SubscriptKind::Literal);
        }

        if is_standard_library_generic(call_path.as_slice())
            || extend_generics
                .iter()
                .map(|target| from_qualified_name(target))
                .any(|target| call_path == target)
        {
            return Some(SubscriptKind::Generic);
        }

        if is_pep_593_generic_type(call_path.as_slice()) {
            return Some(SubscriptKind::PEP593Annotation);
        }

        for module in typing_modules {
            let module_call_path: CallPath = from_unqualified_name(module);
            if call_path.starts_with(&module_call_path) {
                if let Some(member) = call_path.last() {
                    if is_literal_member(member) {
                        return Some(SubscriptKind::Literal);
                    }
                    if is_standard_library_generic_member(member) {
                        return Some(SubscriptKind::Generic);
                    }
                    if is_pep_593_generic_member(member) {
                        return Some(SubscriptKind::PEP593Annotation);
                    }
                }
            }
        }

        None
    })
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ModuleMember {
    /// A builtin symbol, like `"list"`.
    BuiltIn(&'static str),
    /// A module member, like `("collections", "deque")`.
    Member(&'static str, &'static str),
}

impl std::fmt::Display for ModuleMember {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ModuleMember::BuiltIn(name) => std::write!(f, "{name}"),
            ModuleMember::Member(module, member) => std::write!(f, "{module}.{member}"),
        }
    }
}

/// Returns the PEP 585 standard library generic variant for a `typing` module reference, if such
/// a variant exists.
pub fn to_pep585_generic(expr: &Expr, semantic: &SemanticModel) -> Option<ModuleMember> {
    semantic.resolve_call_path(expr).and_then(|call_path| {
        let [module, member] = call_path.as_slice() else {
            return None;
        };
        as_pep_585_generic(module, member).map(|(module, member)| {
            if module.is_empty() {
                ModuleMember::BuiltIn(member)
            } else {
                ModuleMember::Member(module, member)
            }
        })
    })
}

/// Return whether a given expression uses a PEP 585 standard library generic.
pub fn is_pep585_generic(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic.resolve_call_path(expr).is_some_and(|call_path| {
        let [module, name] = call_path.as_slice() else {
            return false;
        };
        has_pep_585_generic(module, name)
    })
}

#[derive(Debug, Copy, Clone)]
pub enum Pep604Operator {
    /// The union operator, e.g., `Union[str, int]`, expressible as `str | int` after PEP 604.
    Union,
    /// The union operator, e.g., `Optional[str]`, expressible as `str | None` after PEP 604.
    Optional,
}

/// Return the PEP 604 operator variant to which the given subscript [`Expr`] corresponds, if any.
pub fn to_pep604_operator(
    value: &Expr,
    slice: &Expr,
    semantic: &SemanticModel,
) -> Option<Pep604Operator> {
    /// Returns `true` if any argument in the slice is a quoted annotation).
    fn quoted_annotation(slice: &Expr) -> bool {
        match slice {
            Expr::Constant(ast::ExprConstant {
                value: Constant::Str(_),
                ..
            }) => true,
            Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().any(quoted_annotation),
            _ => false,
        }
    }

    // If the slice is a forward reference (e.g., `Optional["Foo"]`), it can only be rewritten
    // if we're in a typing-only context.
    //
    // This, for example, is invalid, as Python will evaluate `"Foo" | None` at runtime in order to
    // populate the function's `__annotations__`:
    // ```python
    // def f(x: "Foo" | None): ...
    // ```
    //
    // This, however, is valid:
    // ```python
    // def f():
    //     x: "Foo" | None
    // ```
    if quoted_annotation(slice) {
        if semantic.execution_context().is_runtime() {
            return None;
        }
    }

    semantic
        .resolve_call_path(value)
        .as_ref()
        .and_then(|call_path| {
            if semantic.match_typing_call_path(call_path, "Optional") {
                Some(Pep604Operator::Optional)
            } else if semantic.match_typing_call_path(call_path, "Union") {
                Some(Pep604Operator::Union)
            } else {
                None
            }
        })
}

/// Return `true` if `Expr` represents a reference to a type annotation that resolves to an
/// immutable type.
pub fn is_immutable_annotation(
    expr: &Expr,
    semantic: &SemanticModel,
    extend_immutable_calls: &[CallPath],
) -> bool {
    match expr {
        Expr::Name(_) | Expr::Attribute(_) => {
            semantic.resolve_call_path(expr).is_some_and(|call_path| {
                is_immutable_non_generic_type(call_path.as_slice())
                    || is_immutable_generic_type(call_path.as_slice())
                    || extend_immutable_calls
                        .iter()
                        .any(|target| call_path == *target)
            })
        }
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            semantic.resolve_call_path(value).is_some_and(|call_path| {
                if is_immutable_generic_type(call_path.as_slice()) {
                    true
                } else if matches!(call_path.as_slice(), ["typing", "Union"]) {
                    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                        elts.iter().all(|elt| {
                            is_immutable_annotation(elt, semantic, extend_immutable_calls)
                        })
                    } else {
                        false
                    }
                } else if matches!(call_path.as_slice(), ["typing", "Optional"]) {
                    is_immutable_annotation(slice, semantic, extend_immutable_calls)
                } else if is_pep_593_generic_type(call_path.as_slice()) {
                    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                        elts.first().is_some_and(|elt| {
                            is_immutable_annotation(elt, semantic, extend_immutable_calls)
                        })
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
        }
        Expr::BinOp(ast::ExprBinOp {
            left,
            op: Operator::BitOr,
            right,
            range: _,
        }) => {
            is_immutable_annotation(left, semantic, extend_immutable_calls)
                && is_immutable_annotation(right, semantic, extend_immutable_calls)
        }
        Expr::Constant(ast::ExprConstant {
            value: Constant::None,
            ..
        }) => true,
        _ => false,
    }
}

/// Return `true` if `func` is a function that returns an immutable value.
pub fn is_immutable_func(
    func: &Expr,
    semantic: &SemanticModel,
    extend_immutable_calls: &[CallPath],
) -> bool {
    semantic.resolve_call_path(func).is_some_and(|call_path| {
        is_immutable_return_type(call_path.as_slice())
            || extend_immutable_calls
                .iter()
                .any(|target| call_path == *target)
    })
}

/// Return `true` if `func` is a function that returns a mutable value.
pub fn is_mutable_func(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(func)
        .as_ref()
        .map(CallPath::as_slice)
        .is_some_and(is_mutable_return_type)
}

/// Return `true` if `expr` is an expression that resolves to a mutable value.
pub fn is_mutable_expr(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        Expr::List(_)
        | Expr::Dict(_)
        | Expr::Set(_)
        | Expr::ListComp(_)
        | Expr::DictComp(_)
        | Expr::SetComp(_) => true,
        Expr::Call(ast::ExprCall { func, .. }) => is_mutable_func(func, semantic),
        _ => false,
    }
}

/// Return `true` if [`Expr`] is a guard for a type-checking block.
pub fn is_type_checking_block(stmt: &ast::StmtIf, semantic: &SemanticModel) -> bool {
    let ast::StmtIf { test, .. } = stmt;

    // Ex) `if False:`
    if is_const_false(test) {
        return true;
    }

    // Ex) `if 0:`
    if let Expr::Constant(ast::ExprConstant {
        value: Constant::Int(value),
        ..
    }) = test.as_ref()
    {
        if value.is_zero() {
            return true;
        }
    }

    // Ex) `if typing.TYPE_CHECKING:`
    if semantic
        .resolve_call_path(test)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["typing", "TYPE_CHECKING"]))
    {
        return true;
    }

    false
}

/// Abstraction for a type checker, conservatively checks for the intended type(s).
trait TypeChecker {
    /// Check annotation expression to match the intended type(s).
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool;
    /// Check initializer expression to match the intended type(s).
    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool;
}

/// Check if the type checker accepts the given binding with the given name.
///
/// NOTE: this function doesn't perform more serious type inference, so it won't be able
///       to understand if the value gets initialized from a call to a function always returning
///       lists. This also implies no interfile analysis.
fn check_type<T: TypeChecker>(binding: &Binding, semantic: &SemanticModel) -> bool {
    match binding.kind {
        BindingKind::Assignment => match binding.statement(semantic) {
            // ```python
            // x = init_expr
            // ```
            //
            // The type checker might know how to infer the type based on `init_expr`.
            Some(Stmt::Assign(ast::StmtAssign { value, .. })) => {
                T::match_initializer(value.as_ref(), semantic)
            }

            // ```python
            // x: annotation = some_expr
            // ```
            //
            // In this situation, we check only the annotation.
            Some(Stmt::AnnAssign(ast::StmtAnnAssign { annotation, .. })) => {
                T::match_annotation(annotation.as_ref(), semantic)
            }
            _ => false,
        },

        BindingKind::Argument => match binding.statement(semantic) {
            // ```python
            // def foo(x: annotation):
            //   ...
            // ```
            //
            // We trust the annotation and see if the type checker matches the annotation.
            Some(Stmt::FunctionDef(ast::StmtFunctionDef { parameters, .. })) => {
                let Some(parameter) = find_parameter(parameters.as_ref(), binding) else {
                    return false;
                };
                let Some(ref annotation) = parameter.parameter.annotation else {
                    return false;
                };
                T::match_annotation(annotation.as_ref(), semantic)
            }
            _ => false,
        },

        BindingKind::Annotation => match binding.statement(semantic) {
            // ```python
            // x: annotation
            // ```
            //
            // It's a typed declaration, type annotation is the only source of information.
            Some(Stmt::AnnAssign(ast::StmtAnnAssign { annotation, .. })) => {
                T::match_annotation(annotation.as_ref(), semantic)
            }
            _ => false,
        },

        _ => false,
    }
}

/// Type checker for builtin types.
trait BuiltinTypeChecker {
    /// Builtin type name.
    const BUILTIN_TYPE_NAME: &'static str;
    /// Type name as found in the `Typing` module.
    const TYPING_NAME: &'static str;
    /// [`PythonType`] associated with the intended type.
    const EXPR_TYPE: PythonType;

    /// Check annotation expression to match the intended type.
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
        let value = map_subscript(annotation);
        Self::match_builtin_type(value, semantic)
            || semantic.match_typing_expr(value, Self::TYPING_NAME)
    }

    /// Check initializer expression to match the intended type.
    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool {
        Self::match_expr_type(initializer) || Self::match_builtin_constructor(initializer, semantic)
    }

    /// Check if the type can be inferred from the given expression.
    fn match_expr_type(initializer: &Expr) -> bool {
        let init_type: ResolvedPythonType = initializer.into();
        match init_type {
            ResolvedPythonType::Atom(atom) => atom == Self::EXPR_TYPE,
            _ => false,
        }
    }

    /// Check if the given expression corresponds to a constructor call of the builtin type.
    fn match_builtin_constructor(initializer: &Expr, semantic: &SemanticModel) -> bool {
        let Expr::Call(ast::ExprCall { func, .. }) = initializer else {
            return false;
        };
        Self::match_builtin_type(func.as_ref(), semantic)
    }

    /// Check if the given expression names the builtin type.
    fn match_builtin_type(type_expr: &Expr, semantic: &SemanticModel) -> bool {
        let Expr::Name(ast::ExprName { id, .. }) = type_expr else {
            return false;
        };
        id == Self::BUILTIN_TYPE_NAME && semantic.is_builtin(Self::BUILTIN_TYPE_NAME)
    }
}

impl<T: BuiltinTypeChecker> TypeChecker for T {
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
        <Self as BuiltinTypeChecker>::match_annotation(annotation, semantic)
    }

    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool {
        <Self as BuiltinTypeChecker>::match_initializer(initializer, semantic)
    }
}

struct ListChecker;

impl BuiltinTypeChecker for ListChecker {
    const BUILTIN_TYPE_NAME: &'static str = "list";
    const TYPING_NAME: &'static str = "List";
    const EXPR_TYPE: PythonType = PythonType::List;
}

struct DictChecker;

impl BuiltinTypeChecker for DictChecker {
    const BUILTIN_TYPE_NAME: &'static str = "dict";
    const TYPING_NAME: &'static str = "Dict";
    const EXPR_TYPE: PythonType = PythonType::Dict;
}

struct SetChecker;

impl BuiltinTypeChecker for SetChecker {
    const BUILTIN_TYPE_NAME: &'static str = "set";
    const TYPING_NAME: &'static str = "Set";
    const EXPR_TYPE: PythonType = PythonType::Set;
}

/// Test whether the given binding (and the given name) can be considered a list.
/// For this, we check what value might be associated with it through it's initialization and
/// what annotation it has (we consider `list` and `typing.List`).
pub fn is_list(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<ListChecker>(binding, semantic)
}

/// Test whether the given binding (and the given name) can be considered a dictionary.
/// For this, we check what value might be associated with it through it's initialization and
/// what annotation it has (we consider `dict` and `typing.Dict`).
pub fn is_dict(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<DictChecker>(binding, semantic)
}

/// Test whether the given binding (and the given name) can be considered a set.
/// For this, we check what value might be associated with it through it's initialization and
/// what annotation it has (we consider `set` and `typing.Set`).
pub fn is_set(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<SetChecker>(binding, semantic)
}

/// Find the [`ParameterWithDefault`] corresponding to the given [`Binding`].
#[inline]
fn find_parameter<'a>(
    parameters: &'a Parameters,
    binding: &Binding,
) -> Option<&'a ParameterWithDefault> {
    parameters
        .args
        .iter()
        .chain(parameters.posonlyargs.iter())
        .chain(parameters.kwonlyargs.iter())
        .find(|arg| arg.parameter.name.range() == binding.range())
}
