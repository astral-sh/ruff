use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::{class, function_type};
use ruff_python_semantic::{ScopeKind, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of caching decorators (e.g., `functools.lru_cache`,
/// `functools.cache`) or async caching decorators (e.g., `async_lru.alru_cache`, `aiocache.cached`,
/// `aiocache.cached_stampede`, `aiocache.multi_cached`) on methods.
///
/// ## Why is this bad?
/// Using cache decorators on methods can lead to memory leaks, as the global
/// cache will retain a reference to the instance, preventing it from being
/// garbage collected.
///
/// Instead, refactor the method to depend only on its arguments and not on the
/// instance of the class, or use the decorator on a function outside of the class.
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
/// ## References
/// - [Python documentation: `functools.lru_cache`](https://docs.python.org/3/library/functools.html#functools.lru_cache)
/// - [Python documentation: `functools.cache`](https://docs.python.org/3/library/functools.html#functools.cache)
/// - [Github: `async-lru`](https://github.com/aio-libs/async-lru)
/// - [Github: `aiocache`](https://github.com/aio-libs/aiocache)
/// - [don't lru_cache methods!](https://www.youtube.com/watch?v=sVjtp6tGo0g)
#[derive(ViolationMetadata)]
pub(crate) struct CachedInstanceMethod {
    decorator_name: LruDecorator,
}

impl CachedInstanceMethod {
    pub(crate) fn new(decorator_name: LruDecorator) -> Self {
        Self { decorator_name }
    }
}

impl Violation for CachedInstanceMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use of `{}` on methods can lead to memory leaks",
            self.decorator_name
        )
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum LruDecorator {
    FuncToolsLruCache,
    FunctoolsCache,
    AsyncLru,
    AiocacheCached,
    AiocacheCachedStampede,
    AiocacheMultiCached,
}

impl LruDecorator {
    fn from_qualified_name(qualified_name: &QualifiedName<'_>) -> Option<Self> {
        match qualified_name.segments() {
            ["functools", "lru_cache"] => Some(Self::FuncToolsLruCache),
            ["functools", "cache"] => Some(Self::FunctoolsCache),
            ["async_lru", "alru_cache"] => Some(Self::AsyncLru),
            ["aiocache", "cached"] => Some(Self::AiocacheCached),
            ["aiocache", "cached_stampede"] => Some(Self::AiocacheCachedStampede),
            ["aiocache", "multi_cached"] => Some(Self::AiocacheMultiCached),
            _ => None,
        }
    }
}

impl std::fmt::Display for LruDecorator {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::FuncToolsLruCache => f.write_str("functools.lru_cache"),
            Self::FunctoolsCache => f.write_str("functools.cache"),
            Self::AsyncLru => f.write_str("async_lru.alru_cache"),
            Self::AiocacheCached => f.write_str("aiocache.cached"),
            Self::AiocacheCachedStampede => f.write_str("aiocache.cached_stampede"),
            Self::AiocacheMultiCached => f.write_str("aiocache.multi_cached"),
        }
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
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    );
    if !matches!(type_, function_type::FunctionType::Method) {
        return;
    }

    for decorator in &function_def.decorator_list {
        if let Some(decorator_name) =
            get_cache_decorator_name(map_callable(&decorator.expression), checker.semantic())
        {
            // Ignore if class is an enum (enum members are singletons).
            if class::is_enumeration(class_def, checker.semantic()) {
                return;
            }

            checker.report_diagnostic(Diagnostic::new(
                CachedInstanceMethod::new(decorator_name),
                decorator.range(),
            ));
        }
    }
}

/// Returns `Some(<decorator_name>)` if the given expression is one of the known
/// cache decorators, otherwise `None`.
fn get_cache_decorator_name(expr: &Expr, semantic: &SemanticModel) -> Option<LruDecorator> {
    if let Some(qualified_name) = semantic.resolve_qualified_name(expr) {
        LruDecorator::from_qualified_name(&qualified_name)
    } else {
        None
    }
}
