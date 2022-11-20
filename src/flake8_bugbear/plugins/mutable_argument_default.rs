use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Arguments, Constant, Expr, ExprKind, Operator};

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

const MUTABLE_FUNCS: &[(&str, &str)] = &[
    ("", "dict"),
    ("", "list"),
    ("", "set"),
    ("collections", "Counter"),
    ("collections", "OrderedDict"),
    ("collections", "defaultdict"),
    ("collections", "deque"),
];

const IMMUTABLE_TYPES: &[(&str, &str)] = &[
    ("", "bool"),
    ("", "bytes"),
    ("", "complex"),
    ("", "float"),
    ("", "frozenset"),
    ("", "int"),
    ("", "object"),
    ("", "range"),
    ("", "str"),
    ("collections.abc", "Sized"),
    ("typing", "LiteralString"),
    ("typing", "Sized"),
];

const IMMUTABLE_GENERIC_TYPES: &[(&str, &str)] = &[
    ("", "tuple"),
    ("collections.abc", "ByteString"),
    ("collections.abc", "Collection"),
    ("collections.abc", "Container"),
    ("collections.abc", "Iterable"),
    ("collections.abc", "Mapping"),
    ("collections.abc", "Reversible"),
    ("collections.abc", "Sequence"),
    ("collections.abc", "Set"),
    ("typing", "AbstractSet"),
    ("typing", "ByteString"),
    ("typing", "Callable"),
    ("typing", "Collection"),
    ("typing", "Container"),
    ("typing", "FrozenSet"),
    ("typing", "Iterable"),
    ("typing", "Literal"),
    ("typing", "Mapping"),
    ("typing", "Never"),
    ("typing", "NoReturn"),
    ("typing", "Reversible"),
    ("typing", "Sequence"),
    ("typing", "Tuple"),
];

pub fn is_mutable_func(
    expr: &Expr,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> bool {
    let call_path = dealias_call_path(collect_call_paths(expr), import_aliases);
    MUTABLE_FUNCS
        .iter()
        .any(|(module, member)| match_call_path(&call_path, module, member, from_imports))
}

fn is_mutable_expr(
    expr: &Expr,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> bool {
    match &expr.node {
        ExprKind::List { .. }
        | ExprKind::Dict { .. }
        | ExprKind::Set { .. }
        | ExprKind::ListComp { .. }
        | ExprKind::DictComp { .. }
        | ExprKind::SetComp { .. } => true,
        ExprKind::Call { func, .. } => is_mutable_func(func, from_imports, import_aliases),
        _ => false,
    }
}

fn is_immutable_annotation(
    expr: &Expr,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> bool {
    match &expr.node {
        ExprKind::Name { .. } | ExprKind::Attribute { .. } => {
            let call_path = dealias_call_path(collect_call_paths(expr), import_aliases);
            IMMUTABLE_TYPES
                .iter()
                .chain(IMMUTABLE_GENERIC_TYPES)
                .any(|(module, member)| match_call_path(&call_path, module, member, from_imports))
        }
        ExprKind::Subscript { value, slice, .. } => {
            let call_path = dealias_call_path(collect_call_paths(value), import_aliases);
            if IMMUTABLE_GENERIC_TYPES
                .iter()
                .any(|(module, member)| match_call_path(&call_path, module, member, from_imports))
            {
                true
            } else if match_call_path(&call_path, "typing", "Union", from_imports) {
                if let ExprKind::Tuple { elts, .. } = &slice.node {
                    elts.iter()
                        .all(|elt| is_immutable_annotation(elt, from_imports, import_aliases))
                } else {
                    false
                }
            } else if match_call_path(&call_path, "typing", "Optional", from_imports) {
                is_immutable_annotation(slice, from_imports, import_aliases)
            } else if match_call_path(&call_path, "typing", "Annotated", from_imports) {
                if let ExprKind::Tuple { elts, .. } = &slice.node {
                    elts.first().map_or(false, |elt| {
                        is_immutable_annotation(elt, from_imports, import_aliases)
                    })
                } else {
                    false
                }
            } else {
                false
            }
        }
        ExprKind::BinOp {
            left,
            op: Operator::BitOr,
            right,
        } => {
            is_immutable_annotation(left, from_imports, import_aliases)
                && is_immutable_annotation(right, from_imports, import_aliases)
        }
        ExprKind::Constant {
            value: Constant::None,
            ..
        } => true,
        _ => false,
    }
}

/// B006
pub fn mutable_argument_default(checker: &mut Checker, arguments: &Arguments) {
    // Scan in reverse order to right-align zip()
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
        if is_mutable_expr(default, &checker.from_imports, &checker.import_aliases)
            && arg.node.annotation.as_ref().map_or(true, |expr| {
                !is_immutable_annotation(expr, &checker.from_imports, &checker.import_aliases)
            })
        {
            checker.add_check(Check::new(
                CheckKind::MutableArgumentDefault,
                Range::from_located(default),
            ));
        }
    }
}
