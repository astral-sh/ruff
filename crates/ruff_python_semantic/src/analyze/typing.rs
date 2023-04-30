use rustpython_parser::ast::{Constant, Expr, ExprKind, Operator};

use ruff_python_ast::call_path::{from_unqualified_name, CallPath};
use ruff_python_stdlib::typing::{
    IMMUTABLE_GENERIC_TYPES, IMMUTABLE_TYPES, PEP_585_BUILTINS_ELIGIBLE, PEP_593_SUBSCRIPTS,
    SUBSCRIPTS,
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
    if !matches!(
        expr.node,
        ExprKind::Name { .. } | ExprKind::Attribute { .. }
    ) {
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

/// Returns `true` if `Expr` represents a reference to a typing object with a
/// PEP 585 built-in.
pub fn is_pep585_builtin(expr: &Expr, context: &Context) -> bool {
    context.resolve_call_path(expr).map_or(false, |call_path| {
        PEP_585_BUILTINS_ELIGIBLE.contains(&call_path.as_slice())
    })
}

pub fn is_immutable_annotation(context: &Context, expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Name { .. } | ExprKind::Attribute { .. } => {
            context.resolve_call_path(expr).map_or(false, |call_path| {
                IMMUTABLE_TYPES
                    .iter()
                    .chain(IMMUTABLE_GENERIC_TYPES)
                    .any(|target| call_path.as_slice() == *target)
            })
        }
        ExprKind::Subscript { value, slice, .. } => {
            context.resolve_call_path(value).map_or(false, |call_path| {
                if IMMUTABLE_GENERIC_TYPES
                    .iter()
                    .any(|target| call_path.as_slice() == *target)
                {
                    true
                } else if call_path.as_slice() == ["typing", "Union"] {
                    if let ExprKind::Tuple { elts, .. } = &slice.node {
                        elts.iter().all(|elt| is_immutable_annotation(context, elt))
                    } else {
                        false
                    }
                } else if call_path.as_slice() == ["typing", "Optional"] {
                    is_immutable_annotation(context, slice)
                } else if call_path.as_slice() == ["typing", "Annotated"] {
                    if let ExprKind::Tuple { elts, .. } = &slice.node {
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
        ExprKind::BinOp {
            left,
            op: Operator::BitOr,
            right,
        } => is_immutable_annotation(context, left) && is_immutable_annotation(context, right),
        ExprKind::Constant {
            value: Constant::None,
            ..
        } => true,
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
