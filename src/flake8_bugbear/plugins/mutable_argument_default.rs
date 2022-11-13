use fnv::{FnvHashMap, FnvHashSet};
use rustpython_ast::{Arguments, Expr, ExprKind};

use crate::ast::helpers::{compose_call_path, match_call_path};
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
    compose_call_path(expr)
        .map(|call_path| {
            MUTABLE_FUNCS
                .iter()
                .any(|target| match_call_path(&call_path, target, from_imports))
        })
        .unwrap_or(false)
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
