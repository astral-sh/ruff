use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::{class, function_type};
use ruff_python_semantic::{ScopeKind, SemanticModel};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of method-caching decorators that retain a strong reference
/// to `self`. This includes `functools.lru_cache` and `functools.cache`, as
/// well as the async equivalents `async_lru.alru_cache` and the `aiocache`
/// decorators (`aiocache.cached`, `aiocache.cached_stampede`, and
/// `aiocache.multi_cached`).
///
/// ## Why is this bad?
/// Applying these decorators to methods can lead to memory leaks, as the
/// (global) cache will retain a reference to the instance, preventing it from
/// being garbage collected.
///
/// Instead, refactor the method to depend only on its arguments and not on the
/// instance of the class, or apply the decorator to a function defined outside
/// of the class.
///
/// This rule ignores instance methods on enumeration classes, as enum members
/// are singletons.
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
/// ## Options
///
/// This rule only applies to regular methods, not static or class methods. You can customize how
/// Ruff categorizes methods with the following options:
///
/// - `lint.pep8-naming.classmethod-decorators`
/// - `lint.pep8-naming.staticmethod-decorators`
///
/// ## References
/// - [Python documentation: `functools.lru_cache`](https://docs.python.org/3/library/functools.html#functools.lru_cache)
/// - [Python documentation: `functools.cache`](https://docs.python.org/3/library/functools.html#functools.cache)
/// - [`async_lru` documentation](https://github.com/aio-libs/async-lru)
/// - [`aiocache` documentation](https://aiocache.aio-libs.org/)
/// - [don't lru_cache methods!](https://www.youtube.com/watch?v=sVjtp6tGo0g)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.114")]
pub(crate) struct CachedInstanceMethod;

impl Violation for CachedInstanceMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of caching decorators (e.g. `functools.lru_cache`, `async_lru.alru_cache`, \
         `aiocache.cached`) on methods can lead to memory leaks"
            .to_string()
    }
}

/// B019
pub(crate) fn cached_instance_method(checker: &Checker, function_def: &ast::StmtFunctionDef) {
    let scope = checker.semantic().current_scope();

    // Parent scope _must_ be a class.
    let ScopeKind::Class(class_def) = scope.kind else {
        return;
    };

    // The function must be an _instance_ method.
    let type_ = function_type::classify(
        &function_def.name,
        &function_def.decorator_list,
        scope,
        checker.semantic(),
        &checker.settings().pep8_naming.classmethod_decorators,
        &checker.settings().pep8_naming.staticmethod_decorators,
    );
    if !matches!(type_, function_type::FunctionType::Method) {
        return;
    }

    for decorator in &function_def.decorator_list {
        if is_cache_func(map_callable(&decorator.expression), checker.semantic()) {
            // If we found a cached instance method, validate (lazily) that the class is not an enum.
            if class::is_enumeration(class_def, checker.semantic()) {
                return;
            }

            checker.report_diagnostic(CachedInstanceMethod, decorator.range());
        }
    }
}

/// Returns `true` if the given expression is a call to one of the
/// method-caching decorators handled by `B019`. This currently covers:
///
/// - `functools.lru_cache` / `functools.cache`
/// - `async_lru.alru_cache`
/// - `aiocache.cached` / `aiocache.cached_stampede` / `aiocache.multi_cached`
///
/// All of these store a strong reference to `self` in a (typically global)
/// cache, so applying any of them to an instance method has the same
/// memory-leak failure mode that `B019` was originally written to catch.
fn is_cache_func(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["functools", "lru_cache" | "cache"]
                    | ["async_lru", "alru_cache"]
                    | ["aiocache", "cached" | "cached_stampede" | "multi_cached"]
            )
        })
}
