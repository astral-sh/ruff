use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::model::SemanticModel;
use ruff_python_semantic::scope::ScopeKind;

use crate::checkers::ast::Checker;

#[violation]
pub struct CachedInstanceMethod;

impl Violation for CachedInstanceMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use of `functools.lru_cache` or `functools.cache` on methods can lead to memory leaks"
        )
    }
}

fn is_cache_func(model: &SemanticModel, expr: &Expr) -> bool {
    model.resolve_call_path(expr).map_or(false, |call_path| {
        call_path.as_slice() == ["functools", "lru_cache"]
            || call_path.as_slice() == ["functools", "cache"]
    })
}

/// B019
pub(crate) fn cached_instance_method(checker: &mut Checker, decorator_list: &[Expr]) {
    if !matches!(checker.semantic_model().scope().kind, ScopeKind::Class(_)) {
        return;
    }
    for decorator in decorator_list {
        // TODO(charlie): This should take into account `classmethod-decorators` and
        // `staticmethod-decorators`.
        if let Expr::Name(ast::ExprName { id, .. }) = decorator {
            if id == "classmethod" || id == "staticmethod" {
                return;
            }
        }
    }
    for decorator in decorator_list {
        if is_cache_func(
            checker.semantic_model(),
            match decorator {
                Expr::Call(ast::ExprCall { func, .. }) => func,
                _ => decorator,
            },
        ) {
            checker
                .diagnostics
                .push(Diagnostic::new(CachedInstanceMethod, decorator.range()));
        }
    }
}
