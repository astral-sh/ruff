use ruff_python::typing::{PEP_585_BUILTINS_ELIGIBLE, PEP_593_SUBSCRIPTS, SUBSCRIPTS};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::types::CallPath;

pub enum Callable {
    ForwardRef,
    Cast,
    NewType,
    TypeVar,
    NamedTuple,
    TypedDict,
    MypyExtension,
}

pub enum SubscriptKind {
    AnnotatedSubscript,
    PEP593AnnotatedSubscript,
}

pub fn match_annotated_subscript<'a, F>(
    expr: &'a Expr,
    resolve_call_path: F,
    typing_modules: impl Iterator<Item = &'a str>,
) -> Option<SubscriptKind>
where
    F: FnOnce(&'a Expr) -> Option<CallPath<'a>>,
{
    if !matches!(
        expr.node,
        ExprKind::Name { .. } | ExprKind::Attribute { .. }
    ) {
        return None;
    }

    resolve_call_path(expr).and_then(|call_path| {
        if SUBSCRIPTS.contains(&call_path.as_slice()) {
            return Some(SubscriptKind::AnnotatedSubscript);
        }
        if PEP_593_SUBSCRIPTS.contains(&call_path.as_slice()) {
            return Some(SubscriptKind::PEP593AnnotatedSubscript);
        }

        for module in typing_modules {
            let module_call_path = module.split('.').collect::<Vec<_>>();
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
pub fn is_pep585_builtin<'a, F>(expr: &'a Expr, resolve_call_path: F) -> bool
where
    F: FnOnce(&'a Expr) -> Option<CallPath<'a>>,
{
    resolve_call_path(expr).map_or(false, |call_path| {
        PEP_585_BUILTINS_ELIGIBLE.contains(&call_path.as_slice())
    })
}
