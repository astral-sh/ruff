use rustpython_ast::Expr;

use crate::ast::helpers::compose_call_path;
use crate::ast::types::{Range, ScopeKind};
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

const CACHE_FUNCTIONS: [&str; 4] = [
    "functools.lru_cache",
    "functools.cache",
    "lru_cache",
    "cache",
];

pub fn cached_instance_method(checker: &mut Checker, decorator_list: &[Expr]) {
    if matches!(checker.current_scope().kind, ScopeKind::Class(_)) {
        for decorator in decorator_list {
            if let Some(decorator_path) = compose_call_path(decorator) {
                if decorator_path == "classmethod" || decorator_path == "staticmethod" {
                    return;
                }
                if CACHE_FUNCTIONS.contains(&decorator_path.as_str()) {
                    checker.add_check(Check::new(
                        CheckKind::CachedInstanceMethod,
                        Range::from_located(decorator),
                    ));
                }
            }
        }
    }
}
