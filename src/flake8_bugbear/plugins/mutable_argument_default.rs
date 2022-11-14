use fnv::{FnvHashMap, FnvHashSet};
use rustpython_ast::{Arguments, Expr, ExprKind};

<<<<<<< HEAD
use crate::ast::helpers::{collect_call_paths, match_call_path};
=======
use crate::ast::helpers::{compose_call_path, dealias, match_call_path};
>>>>>>> 4b06237 (Track aliases)
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

<<<<<<< HEAD
pub fn is_mutable_func(expr: &Expr, from_imports: &FnvHashMap<&str, FnvHashSet<&str>>) -> bool {
    let call_path = collect_call_paths(expr);
    MUTABLE_FUNCS
        .iter()
        .any(|(module, member)| match_call_path(&call_path, module, member, from_imports))
=======
pub fn is_mutable_func(
    expr: &Expr,
    from_imports: &FnvHashMap<&str, FnvHashSet<&str>>,
    import_aliases: &FnvHashMap<&str, &str>,
) -> bool {
    compose_call_path(expr)
        .map(|call_path| dealias(call_path, import_aliases))
        .map(|call_path| {
            MUTABLE_FUNCS
                .iter()
                .any(|target| match_call_path(&call_path, target, from_imports))
        })
        .unwrap_or(false)
>>>>>>> 4b06237 (Track aliases)
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
