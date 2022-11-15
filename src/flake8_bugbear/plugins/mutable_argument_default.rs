use fnv::{FnvHashMap, FnvHashSet};
use rustpython_ast::{Arguments, Expr, ExprKind};

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

const MUTABLE_FUNCS: [(&str, &str); 7] = [
    ("", "dict"),
    ("", "list"),
    ("", "set"),
    ("collections", "Counter"),
    ("collections", "OrderedDict"),
    ("collections", "defaultdict"),
    ("collections", "deque"),
];

pub fn is_mutable_func(
    expr: &Expr,
    from_imports: &FnvHashMap<&str, FnvHashSet<&str>>,
    import_aliases: &FnvHashMap<&str, &str>,
) -> bool {
    let call_path = dealias_call_path(collect_call_paths(expr), import_aliases);
    MUTABLE_FUNCS
        .iter()
        .any(|(module, member)| match_call_path(&call_path, module, member, from_imports))
}

/// B006
pub fn mutable_argument_default(checker: &mut Checker, arguments: &Arguments) {
    for expr in arguments
        .defaults
        .iter()
        .chain(arguments.kw_defaults.iter())
    {
        match &expr.node {
            ExprKind::List { .. }
            | ExprKind::Dict { .. }
            | ExprKind::Set { .. }
            | ExprKind::ListComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::SetComp { .. } => {
                checker.add_check(Check::new(
                    CheckKind::MutableArgumentDefault,
                    Range::from_located(expr),
                ));
            }
            ExprKind::Call { func, .. } => {
                if is_mutable_func(func, &checker.from_imports, &checker.import_aliases) {
                    checker.add_check(Check::new(
                        CheckKind::MutableArgumentDefault,
                        Range::from_located(expr),
                    ));
                }
            }
            _ => {}
        }
    }
}
