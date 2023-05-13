use rustpython_parser::ast::{self, Constant, Expr, ExprKind, Operator};

use ruff_python_ast::call_path::{from_unqualified_name, CallPath};
use ruff_python_stdlib::typing::{
    IMMUTABLE_GENERIC_TYPES, IMMUTABLE_TYPES, PEP_585_GENERICS, PEP_593_SUBSCRIPTS, SUBSCRIPTS,
};

use crate::context::Context;

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
    context: &Context,
    typing_modules: impl Iterator<Item = &'a str>,
) -> Option<SubscriptKind> {
    if !matches!(expr.node, ExprKind::Name(_) | ExprKind::Attribute(_)) {
        return None;
    }

    context.resolve_call_path(expr).and_then(|call_path| {
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

/// A module member, like `("typing", "Deque")`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ModuleMember<'a> {
    /// The module name, like `"typing"`.
    module: &'a str,
    /// The member name, like `"Deque"`.
    member: &'a str,
}

impl ModuleMember<'_> {
    /// Returns the module name, like `"typing"`.
    pub fn module(&self) -> &str {
        self.module
    }

    /// Returns the member name, like `"Deque"`.
    pub fn member(&self) -> &str {
        self.member
    }

    /// Returns `true` if this is a builtin symbol, like `int`.
    pub fn is_builtin(&self) -> bool {
        self.module.is_empty()
    }
}

impl std::fmt::Display for ModuleMember<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_builtin() {
            std::write!(f, "{}", self.member)
        } else {
            std::write!(
                f,
                "{module}.{member}",
                module = self.module,
                member = self.member
            )
        }
    }
}

/// A symbol replacement, like `(("typing", "Deque"), ("collections", "deque"))`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SymbolReplacement<'a> {
    /// The symbol to replace, like `("typing", "Deque")`.
    pub from: ModuleMember<'a>,
    /// The symbol to replace with, like `("collections", "deque")`.
    pub to: ModuleMember<'a>,
}

/// Returns the PEP 585 standard library generic variant for a `typing` module reference, if such
/// a variant exists.
pub fn to_pep585_generic(expr: &Expr, context: &Context) -> Option<SymbolReplacement<'static>> {
    context.resolve_call_path(expr).and_then(|call_path| {
        let [module, name] = call_path.as_slice() else {
            return None;
        };
        PEP_585_GENERICS
            .iter()
            .find_map(|((from_module, from_member), (to_module, to_member))| {
                if module == from_module && name == from_member {
                    Some(SymbolReplacement {
                        from: ModuleMember {
                            module: from_module,
                            member: from_member,
                        },
                        to: ModuleMember {
                            module: to_module,
                            member: to_member,
                        },
                    })
                } else {
                    None
                }
            })
    })
}

/// Returns `true` if `Expr` represents a reference to a typing object with a
/// PEP 603 built-in.
pub fn is_pep604_builtin(expr: &Expr, context: &Context) -> bool {
    context.resolve_call_path(expr).map_or(false, |call_path| {
        context.match_typing_call_path(&call_path, "Optional")
            || context.match_typing_call_path(&call_path, "Union")
    })
}

pub fn is_immutable_annotation(context: &Context, expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Name(_) | ExprKind::Attribute(_) => {
            context.resolve_call_path(expr).map_or(false, |call_path| {
                IMMUTABLE_TYPES
                    .iter()
                    .chain(IMMUTABLE_GENERIC_TYPES)
                    .any(|target| call_path.as_slice() == *target)
            })
        }
        ExprKind::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            context.resolve_call_path(value).map_or(false, |call_path| {
                if IMMUTABLE_GENERIC_TYPES
                    .iter()
                    .any(|target| call_path.as_slice() == *target)
                {
                    true
                } else if call_path.as_slice() == ["typing", "Union"] {
                    if let ExprKind::Tuple(ast::ExprTuple { elts, .. }) = &slice.node {
                        elts.iter().all(|elt| is_immutable_annotation(context, elt))
                    } else {
                        false
                    }
                } else if call_path.as_slice() == ["typing", "Optional"] {
                    is_immutable_annotation(context, slice)
                } else if call_path.as_slice() == ["typing", "Annotated"] {
                    if let ExprKind::Tuple(ast::ExprTuple { elts, .. }) = &slice.node {
                        elts.first()
                            .map_or(false, |elt| is_immutable_annotation(context, elt))
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
        }
        ExprKind::BinOp(ast::ExprBinOp {
            left,
            op: Operator::BitOr,
            right,
        }) => is_immutable_annotation(context, left) && is_immutable_annotation(context, right),
        ExprKind::Constant(ast::ExprConstant {
            value: Constant::None,
            ..
        }) => true,
        _ => false,
    }
}

const IMMUTABLE_FUNCS: &[&[&str]] = &[
    &["", "tuple"],
    &["", "frozenset"],
    &["datetime", "date"],
    &["datetime", "datetime"],
    &["datetime", "timedelta"],
    &["decimal", "Decimal"],
    &["operator", "attrgetter"],
    &["operator", "itemgetter"],
    &["operator", "methodcaller"],
    &["pathlib", "Path"],
    &["types", "MappingProxyType"],
    &["re", "compile"],
];

pub fn is_immutable_func(
    context: &Context,
    func: &Expr,
    extend_immutable_calls: &[CallPath],
) -> bool {
    context.resolve_call_path(func).map_or(false, |call_path| {
        IMMUTABLE_FUNCS
            .iter()
            .any(|target| call_path.as_slice() == *target)
            || extend_immutable_calls
                .iter()
                .any(|target| call_path == *target)
    })
}
