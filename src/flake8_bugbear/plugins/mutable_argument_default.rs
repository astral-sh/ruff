use fnv::{FnvHashMap, FnvHashSet};
use rustpython_ast::{Arguments, Expr, ExprKind};

use crate::ast::helpers::compose_call_path;
use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

const MUTABLE_FUNCS: [&str; 7] = [
    "dict",
    "list",
    "set",
    "collections.Counter",
    "collections.OrderedDict",
    "collections.defaultdict",
    "collections.deque",
];

pub fn is_mutable_func(expr: &Expr, from_imports: &FnvHashMap<&str, FnvHashSet<&str>>) -> bool {
    compose_call_path(expr).map_or_else(
        || false,
        |call_path| {
            // It matches the call path exactly (`collections.Counter`).
            for target in MUTABLE_FUNCS {
                if call_path == target {
                    return true;
                }
            }

            // It matches the member name, and was imported from that module (`Counter`
            // following `from collections import Counter`).
            if !call_path.contains('.') {
                for target in MUTABLE_FUNCS {
                    let mut splitter = target.rsplit('.');
                    if let (Some(member), Some(module)) = (splitter.next(), splitter.next()) {
                        if call_path == member
                            && from_imports
                                .get(module)
                                .map(|module| module.contains(member))
                                .unwrap_or(false)
                        {
                            return true;
                        }
                    }
                }
            }

            false
        },
    )
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
                if is_mutable_func(func, &checker.from_imports) {
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
