use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::{class, function_type};
use ruff_python_semantic::{ScopeKind, SemanticModel};
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
/// - [don't lru_cache methods!](https://www.youtube.com/watch?v=sVjtp6tGo0g)
#[derive(ViolationMetadata)]
pub(crate) struct CachedInstanceMethod;

impl Violation for CachedInstanceMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of `functools.lru_cache` or `functools.cache` on methods can lead to memory leaks"
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
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
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

            checker.report_diagnostic(Diagnostic::new(CachedInstanceMethod, decorator.range()));
        }
    }
}

/// Returns `true` if the given expression is a call to `functools.lru_cache` or `functools.cache`.
fn is_cache_func(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["functools", "lru_cache" | "cache"]
            )
        })
}
