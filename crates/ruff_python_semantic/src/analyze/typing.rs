use rustpython_parser::ast::{self, Constant, Expr, Operator};

use num_traits::identities::Zero;
use ruff_python_ast::call_path::{from_unqualified_name, CallPath};
use ruff_python_stdlib::typing::{
    IMMUTABLE_GENERIC_TYPES, IMMUTABLE_TYPES, PEP_585_GENERICS, PEP_593_SUBSCRIPTS, SUBSCRIPTS,
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
    semantic_model: &SemanticModel,
    typing_modules: impl Iterator<Item = &'a str>,
) -> Option<SubscriptKind> {
    if !matches!(expr, Expr::Name(_) | Expr::Attribute(_)) {
        return None;
    }

    semantic_model
        .resolve_call_path(expr)
        .and_then(|call_path| {
            if SUBSCRIPTS.contains(&call_path.as_slice()) {
                return Some(SubscriptKind::AnnotatedSubscript);
            }
            if PEP_593_SUBSCRIPTS.contains(&call_path.as_slice()) {
                return Some(SubscriptKind::PEP593AnnotatedSubscript);
            }

            for module in typing_modules {
                let module_call_path: CallPath = from_unqualified_name(module);
                if call_path.starts_with(&module_call_path) {
                    for subscript in SUBSCRIPTS.iter() {
                        if call_path.last() == subscript.last() {
                            return Some(SubscriptKind::AnnotatedSubscript);
                        }
                    }
                    for subscript in PEP_593_SUBSCRIPTS.iter() {
                        if call_path.last() == subscript.last() {
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
pub fn to_pep585_generic(expr: &Expr, semantic_model: &SemanticModel) -> Option<ModuleMember> {
    semantic_model
        .resolve_call_path(expr)
        .and_then(|call_path| {
            let [module, name] = call_path.as_slice() else {
            return None;
        };
            PEP_585_GENERICS.iter().find_map(
                |((from_module, from_member), (to_module, to_member))| {
                    if module == from_module && name == from_member {
                        if to_module.is_empty() {
                            Some(ModuleMember::BuiltIn(to_member))
                        } else {
                            Some(ModuleMember::Member(to_module, to_member))
                        }
                    } else {
                        None
                    }
                },
            )
        })
}

/// Return whether a given expression uses a PEP 585 standard library generic.
pub fn is_pep585_generic(expr: &Expr, model: &SemanticModel) -> bool {
    if let Some(call_path) = model.resolve_call_path(expr) {
        let [module, name] = call_path.as_slice() else {
            return false;
        };
        for (_, (to_module, to_member)) in PEP_585_GENERICS {
            if module == to_module && name == to_member {
                return true;
            }
        }
    }
    false
}

/// Return whether a given expression uses a PEP 585 standard library generic.
pub fn is_pep585_generic(expr: &Expr, model: &SemanticModel) -> bool {
    if let Some(call_path) = model.resolve_call_path(expr) {
        let [module, name] = call_path.as_slice() else {
            return false;
        };
        for (_, (to_module, to_member)) in PEP_585_GENERICS {
            if module == to_module && name == to_member {
                return true;
            }
        }
    }
    false
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
    semantic_model: &SemanticModel,
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

    semantic_model
        .resolve_call_path(value)
        .as_ref()
        .and_then(|call_path| {
            if semantic_model.match_typing_call_path(call_path, "Optional") {
                Some(Pep604Operator::Optional)
            } else if semantic_model.match_typing_call_path(call_path, "Union") {
                Some(Pep604Operator::Union)
            } else {
                None
            }
        })
}

/// Return `true` if `Expr` represents a reference to a type annotation that resolves to an
/// immutable type.
pub fn is_immutable_annotation(semantic_model: &SemanticModel, expr: &Expr) -> bool {
    match expr {
        Expr::Name(_) | Expr::Attribute(_) => {
            semantic_model
                .resolve_call_path(expr)
                .map_or(false, |call_path| {
                    IMMUTABLE_TYPES
                        .iter()
                        .chain(IMMUTABLE_GENERIC_TYPES)
                        .any(|target| call_path.as_slice() == *target)
                })
        }
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => semantic_model
            .resolve_call_path(value)
            .map_or(false, |call_path| {
                if IMMUTABLE_GENERIC_TYPES
                    .iter()
                    .any(|target| call_path.as_slice() == *target)
                {
                    true
                } else if call_path.as_slice() == ["typing", "Union"] {
                    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                        elts.iter()
                            .all(|elt| is_immutable_annotation(semantic_model, elt))
                    } else {
                        false
                    }
                } else if call_path.as_slice() == ["typing", "Optional"] {
                    is_immutable_annotation(semantic_model, slice)
                } else if call_path.as_slice() == ["typing", "Annotated"] {
                    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                        elts.first()
                            .map_or(false, |elt| is_immutable_annotation(semantic_model, elt))
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
        }) => {
            is_immutable_annotation(semantic_model, left)
                && is_immutable_annotation(semantic_model, right)
        }
        Expr::Constant(ast::ExprConstant {
            value: Constant::None,
            ..
        }) => true,
        _ => false,
    }
}

const IMMUTABLE_FUNCS: &[&[&str]] = &[
    &["", "bool"],
    &["", "complex"],
    &["", "float"],
    &["", "frozenset"],
    &["", "int"],
    &["", "str"],
    &["", "tuple"],
    &["datetime", "date"],
    &["datetime", "datetime"],
    &["datetime", "timedelta"],
    &["decimal", "Decimal"],
    &["fractions", "Fraction"],
    &["operator", "attrgetter"],
    &["operator", "itemgetter"],
    &["operator", "methodcaller"],
    &["pathlib", "Path"],
    &["types", "MappingProxyType"],
    &["re", "compile"],
];

/// Return `true` if `func` is a function that returns an immutable object.
pub fn is_immutable_func(
    semantic_model: &SemanticModel,
    func: &Expr,
    extend_immutable_calls: &[CallPath],
) -> bool {
    semantic_model
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            IMMUTABLE_FUNCS
                .iter()
                .any(|target| call_path.as_slice() == *target)
                || extend_immutable_calls
                    .iter()
                    .any(|target| call_path == *target)
        })
}

/// Return `true` if [`Expr`] is a guard for a type-checking block.
pub fn is_type_checking_block(stmt: &ast::StmtIf, semantic_model: &SemanticModel) -> bool {
    let ast::StmtIf { test, .. } = stmt;

    // Ex) `if False:`
    if matches!(
        test.as_ref(),
        Expr::Constant(ast::ExprConstant {
            value: Constant::Bool(false),
            ..
        })
    ) {
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
    if semantic_model
        .resolve_call_path(test)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["typing", "TYPE_CHECKING"]
        })
    {
        return true;
    }

    false
}
