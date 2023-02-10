use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Arguments, Constant, Expr, ExprKind, Operator};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct MutableArgumentDefault;
);
impl Violation for MutableArgumentDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use mutable data structures for argument defaults")
    }
}
const MUTABLE_FUNCS: &[&[&str]] = &[
    &["", "dict"],
    &["", "list"],
    &["", "set"],
    &["collections", "Counter"],
    &["collections", "OrderedDict"],
    &["collections", "defaultdict"],
    &["collections", "deque"],
];

const IMMUTABLE_TYPES: &[&[&str]] = &[
    &["", "bool"],
    &["", "bytes"],
    &["", "complex"],
    &["", "float"],
    &["", "frozenset"],
    &["", "int"],
    &["", "object"],
    &["", "range"],
    &["", "str"],
    &["collections", "abc", "Sized"],
    &["typing", "LiteralString"],
    &["typing", "Sized"],
];

const IMMUTABLE_GENERIC_TYPES: &[&[&str]] = &[
    &["", "tuple"],
    &["collections", "abc", "ByteString"],
    &["collections", "abc", "Collection"],
    &["collections", "abc", "Container"],
    &["collections", "abc", "Iterable"],
    &["collections", "abc", "Mapping"],
    &["collections", "abc", "Reversible"],
    &["collections", "abc", "Sequence"],
    &["collections", "abc", "Set"],
    &["typing", "AbstractSet"],
    &["typing", "ByteString"],
    &["typing", "Callable"],
    &["typing", "Collection"],
    &["typing", "Container"],
    &["typing", "FrozenSet"],
    &["typing", "Iterable"],
    &["typing", "Literal"],
    &["typing", "Mapping"],
    &["typing", "Never"],
    &["typing", "NoReturn"],
    &["typing", "Reversible"],
    &["typing", "Sequence"],
    &["typing", "Tuple"],
];

pub fn is_mutable_func(checker: &Checker, func: &Expr) -> bool {
    checker.resolve_call_path(func).map_or(false, |call_path| {
        MUTABLE_FUNCS
            .iter()
            .any(|target| call_path.as_slice() == *target)
    })
}

fn is_mutable_expr(checker: &Checker, expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::List { .. }
        | ExprKind::Dict { .. }
        | ExprKind::Set { .. }
        | ExprKind::ListComp { .. }
        | ExprKind::DictComp { .. }
        | ExprKind::SetComp { .. } => true,
        ExprKind::Call { func, .. } => is_mutable_func(checker, func),
        _ => false,
    }
}

fn is_immutable_annotation(checker: &Checker, expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Name { .. } | ExprKind::Attribute { .. } => {
            checker.resolve_call_path(expr).map_or(false, |call_path| {
                IMMUTABLE_TYPES
                    .iter()
                    .chain(IMMUTABLE_GENERIC_TYPES)
                    .any(|target| call_path.as_slice() == *target)
            })
        }
        ExprKind::Subscript { value, slice, .. } => {
            checker.resolve_call_path(value).map_or(false, |call_path| {
                if IMMUTABLE_GENERIC_TYPES
                    .iter()
                    .any(|target| call_path.as_slice() == *target)
                {
                    true
                } else if call_path.as_slice() == ["typing", "Union"] {
                    if let ExprKind::Tuple { elts, .. } = &slice.node {
                        elts.iter().all(|elt| is_immutable_annotation(checker, elt))
                    } else {
                        false
                    }
                } else if call_path.as_slice() == ["typing", "Optional"] {
                    is_immutable_annotation(checker, slice)
                } else if call_path.as_slice() == ["typing", "Annotated"] {
                    if let ExprKind::Tuple { elts, .. } = &slice.node {
                        elts.first()
                            .map_or(false, |elt| is_immutable_annotation(checker, elt))
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
        } => is_immutable_annotation(checker, left) && is_immutable_annotation(checker, right),
        ExprKind::Constant {
            value: Constant::None,
            ..
        } => true,
        _ => false,
    }
}

/// B006
pub fn mutable_argument_default(checker: &mut Checker, arguments: &Arguments) {
    // Scan in reverse order to right-align zip().
    for (arg, default) in arguments
        .kwonlyargs
        .iter()
        .rev()
        .zip(arguments.kw_defaults.iter().rev())
        .chain(
            arguments
                .args
                .iter()
                .rev()
                .chain(arguments.posonlyargs.iter().rev())
                .zip(arguments.defaults.iter().rev()),
        )
    {
        if is_mutable_expr(checker, default)
            && !arg
                .node
                .annotation
                .as_ref()
                .map_or(false, |expr| is_immutable_annotation(checker, expr))
        {
            checker.diagnostics.push(Diagnostic::new(
                MutableArgumentDefault,
                Range::from_located(default),
            ));
        }
    }
}
