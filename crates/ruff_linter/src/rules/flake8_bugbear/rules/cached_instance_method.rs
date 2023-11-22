use ruff_python_ast::{self as ast, Decorator, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of the `functools.lru_cache` and `functools.cache`
/// decorators on methods.
///
/// ## Why is this bad?
/// Using the `functools.lru_cache` and `functools.cache` decorators on methods
/// can lead to memory leaks, as the global cache will retain a reference to
/// the instance, preventing it from being garbage collected.
///
/// Instead, refactor the method to depend only on its arguments and not on the
/// instance of the class, or use the `@lru_cache` decorator on a function
/// outside of the class.
///
/// ## Example
/// ```python
/// from functools import lru_cache
///
///
/// def square(x: int) -> int:
///     return x * x
///
///
/// class Number:
///     value: int
///
///     @lru_cache
///     def squared(self):
///         return square(self.value)
/// ```
///
/// Use instead:
/// ```python
/// from functools import lru_cache
///
///
/// @lru_cache
/// def square(x: int) -> int:
///     return x * x
///
///
/// class Number:
///     value: int
///
///     def squared(self):
///         return square(self.value)
/// ```
///
/// ## References
/// - [Python documentation: `functools.lru_cache`](https://docs.python.org/3/library/functools.html#functools.lru_cache)
/// - [Python documentation: `functools.cache`](https://docs.python.org/3/library/functools.html#functools.cache)
/// - [don't lru_cache methods!](https://www.youtube.com/watch?v=sVjtp6tGo0g)
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

fn is_cache_func(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic.resolve_call_path(expr).is_some_and(|call_path| {
        matches!(call_path.as_slice(), ["functools", "lru_cache" | "cache"])
    })
}

/// B019
pub(crate) fn cached_instance_method(checker: &mut Checker, decorator_list: &[Decorator]) {
    if !checker.semantic().current_scope().kind.is_class() {
        return;
    }
    for decorator in decorator_list {
        // TODO(charlie): This should take into account `classmethod-decorators` and
        // `staticmethod-decorators`.
        if let Expr::Name(ast::ExprName { id, .. }) = &decorator.expression {
            if id == "classmethod" || id == "staticmethod" {
                return;
            }
        }
    }
    for decorator in decorator_list {
        if is_cache_func(
            match &decorator.expression {
                Expr::Call(ast::ExprCall { func, .. }) => func,
                _ => &decorator.expression,
            },
            checker.semantic(),
        ) {
            checker
                .diagnostics
                .push(Diagnostic::new(CachedInstanceMethod, decorator.range()));
        }
    }
}
