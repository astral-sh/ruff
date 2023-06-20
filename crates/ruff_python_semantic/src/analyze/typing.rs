//! Analysis rules for the `typing` module.

use num_traits::identities::Zero;
use rustpython_parser::ast::{self, Constant, Expr, Operator};

use ruff_python_ast::call_path::{from_qualified_name, from_unqualified_name, CallPath};
use ruff_python_ast::helpers::is_const_false;
use ruff_python_stdlib::typing::{
    as_pep_585_generic, has_pep_585_generic, is_immutable_generic_type,
    is_immutable_non_generic_type, is_immutable_return_type, is_mutable_return_type,
    is_pep_593_generic_member, is_pep_593_generic_type, is_standard_library_generic,
    is_standard_library_generic_member,
};

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
    AnnotatedSubscript,
    PEP593AnnotatedSubscript,
}

pub fn match_annotated_subscript<'a>(
    expr: &Expr,
    semantic: &SemanticModel,
    typing_modules: impl Iterator<Item = &'a str>,
    extend_generics: &[String],
) -> Option<SubscriptKind> {
    semantic.resolve_call_path(expr).and_then(|call_path| {
        if is_standard_library_generic(call_path.as_slice())
            || extend_generics
                .iter()
                .map(|target| from_qualified_name(target))
                .any(|target| call_path == target)
        {
            return Some(SubscriptKind::AnnotatedSubscript);
        }

        if is_pep_593_generic_type(call_path.as_slice()) {
            return Some(SubscriptKind::PEP593AnnotatedSubscript);
        }

        for module in typing_modules {
            let module_call_path: CallPath = from_unqualified_name(module);
            if call_path.starts_with(&module_call_path) {
                if let Some(member) = call_path.last() {
                    if is_standard_library_generic_member(member) {
                        return Some(SubscriptKind::AnnotatedSubscript);
                    }
                    if is_pep_593_generic_member(member) {
                        return Some(SubscriptKind::PEP593AnnotatedSubscript);
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
    semantic.resolve_call_path(expr).map_or(false, |call_path| {
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
    /// Returns `true` if any argument in the slice is a string.
    fn any_arg_is_str(slice: &Expr) -> bool {
        match slice {
            Expr::Constant(ast::ExprConstant {
                value: Constant::Str(_),
                ..
            }) => true,
            Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().any(any_arg_is_str),
            _ => false,
        }
    }

    // If any of the _arguments_ are forward references, we can't use PEP 604.
    // Ex) `Union["str", "int"]` can't be converted to `"str" | "int"`.
    if any_arg_is_str(slice) {
        return None;
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
pub fn is_immutable_annotation(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        Expr::Name(_) | Expr::Attribute(_) => {
            semantic.resolve_call_path(expr).map_or(false, |call_path| {
                is_immutable_non_generic_type(call_path.as_slice())
                    || is_immutable_generic_type(call_path.as_slice())
            })
        }
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => semantic
            .resolve_call_path(value)
            .map_or(false, |call_path| {
                if is_immutable_generic_type(call_path.as_slice()) {
                    true
                } else if matches!(call_path.as_slice(), ["typing", "Union"]) {
                    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                        elts.iter()
                            .all(|elt| is_immutable_annotation(elt, semantic))
                    } else {
                        false
                    }
                } else if matches!(call_path.as_slice(), ["typing", "Optional"]) {
                    is_immutable_annotation(slice, semantic)
                } else if matches!(call_path.as_slice(), ["typing", "Annotated"]) {
                    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                        elts.first()
                            .map_or(false, |elt| is_immutable_annotation(elt, semantic))
                    } else {
                        false
                    }
                } else {
                    false
                }
            }),
        Expr::BinOp(ast::ExprBinOp {
            left,
            op: Operator::BitOr,
            right,
            range: _range,
        }) => is_immutable_annotation(left, semantic) && is_immutable_annotation(right, semantic),
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
    semantic.resolve_call_path(func).map_or(false, |call_path| {
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
        .map_or(false, is_mutable_return_type)
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
    if semantic.resolve_call_path(test).map_or(false, |call_path| {
        matches!(call_path.as_slice(), ["typing", "TYPE_CHECKING"])
    }) {
        return true;
    }

    false
}
