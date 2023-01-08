use rustpython_ast::{Expr, ExprKind};

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::{Range, ScopeKind};
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

fn is_cache_func(xxxxxxxx: &xxxxxxxx, expr: &Expr) -> bool {
    let call_path = dealias_call_path(collect_call_paths(expr), &xxxxxxxx.import_aliases);
    match_call_path(&call_path, "functools", "lru_cache", &xxxxxxxx.from_imports)
        || match_call_path(&call_path, "functools", "cache", &xxxxxxxx.from_imports)
}

/// B019
pub fn cached_instance_method(xxxxxxxx: &mut xxxxxxxx, decorator_list: &[Expr]) {
    if !matches!(xxxxxxxx.current_scope().kind, ScopeKind::Class(_)) {
        return;
    }
    for decorator in decorator_list {
        // TODO(charlie): This should take into account `classmethod-decorators` and
        // `staticmethod-decorators`.
        if let ExprKind::Name { id, .. } = &decorator.node {
            if id == "classmethod" || id == "staticmethod" {
                return;
            }
        }
    }
    for decorator in decorator_list {
        if is_cache_func(
            xxxxxxxx,
            match &decorator.node {
                ExprKind::Call { func, .. } => func,
                _ => decorator,
            },
        ) {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::CachedInstanceMethod,
                Range::from_located(decorator),
            ));
        }
    }
}
