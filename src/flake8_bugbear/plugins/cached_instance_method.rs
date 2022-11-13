use rustpython_ast::{Expr, ExprKind};

use crate::ast::helpers::{compose_call_path, match_module_member};
use crate::ast::types::{Range, ScopeKind};
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

fn is_cache_func(checker: &Checker, expr: &Expr) -> bool {
    match_module_member(expr, "functools.lru_cache", &checker.from_imports)
        || match_module_member(expr, "functools.cache", &checker.from_imports)
}

/// B019
pub fn cached_instance_method(checker: &mut Checker, decorator_list: &[Expr]) {
    if matches!(checker.current_scope().kind, ScopeKind::Class(_)) {
        for decorator in decorator_list {
            // TODO(charlie): This should take into account `classmethod-decorators` and
            // `staticmethod-decorators`.
            if let Some(decorator_path) = compose_call_path(decorator) {
                if decorator_path == "classmethod" || decorator_path == "staticmethod" {
                    return;
                }
            }
        }
        for decorator in decorator_list {
            if is_cache_func(
                checker,
                match &decorator.node {
                    ExprKind::Call { func, .. } => func,
                    _ => decorator,
                },
            ) {
                checker.add_check(Check::new(
                    CheckKind::CachedInstanceMethod,
                    Range::from_located(decorator),
                ));
            }
        }
    }
}
