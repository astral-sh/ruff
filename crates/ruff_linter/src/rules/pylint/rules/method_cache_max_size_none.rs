use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::Scope;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `@lru_cache(maxsize=None)` or `@cache` decorators on class and instance methods.
///
/// ## Why is this bad?
/// By decorating a method with `lru_cache` or `cache` the 'self' argument will be linked to
/// the function and therefore never garbage collected.
/// Unless your instance will never need to be garbage collected.
///
/// It is recommended to refactor code to avoid this pattern or add a maxsize to the cache.
///
/// ## Example
/// ```python
/// import functools
///
///
/// class Fibonnaci:
///     @functools.lru_cache(maxsize=None)  # [method-cache-max-size-none]
///     def fibonacci(self, n):
///         if n in {0, 1}:
///             return n
///         return self.fibonacci(n - 1) + self.fibonacci(n - 2)
/// ```
///
/// Use instead:
/// ```python
/// ``import functools
///
///
/// @functools.cache
/// def cached_fibonacci(n):
///     if n in {0, 1}:
///         return n
///     return cached_fibonacci(n - 1) + cached_fibonacci(n - 2)
///
///
/// class Fibonnaci:
///     def fibonacci(self, n):
///         return cached_fibonacci(n)
///
#[violation]
pub struct MethodCacheMaxSizeNone;

impl Violation for MethodCacheMaxSizeNone {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Method decorated with `@lru_cache(maxsize=None)` or `@cache` is never garbage collected.")
    }
}

/// W1518
pub(crate) fn method_cache_size_none(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(func) = scope.kind.as_function() else {
        return;
    };

    let ast::StmtFunctionDef {
        name,
        decorator_list,
        ..
    } = func;

    let Some(parent) = &checker.semantic().first_non_type_parent_scope(scope) else {
        return;
    };

    let type_ = function_type::classify(
        name,
        decorator_list,
        parent,
        checker.semantic(),
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    );
    if !matches!(type_, function_type::FunctionType::Method) {
        return;
    }

    for decorator in decorator_list {
        let decorator_expression = match &decorator.expression {
            ast::Expr::Call(call) => &call.func,
            _ => &decorator.expression,
        };
        let qualified_name = match checker
            .semantic()
            .resolve_qualified_name(decorator_expression)
        {
            Some(qualified_name) => qualified_name,
            None => continue,
        };
        if matches!(qualified_name.segments(), ["functools", "cache"]) {
            let diagnostic = Diagnostic::new(MethodCacheMaxSizeNone, decorator.range());
            diagnostics.push(diagnostic);
        }

        if matches!(qualified_name.segments(), ["functools", "lru_cache"]) {
            let Some(call_expr) = decorator.expression.as_call_expr() else {
                continue;
            };
            let Some(maxsize) =
                call_expr
                    .arguments
                    .keywords
                    .iter()
                    .find(|keyword| match keyword.arg {
                        Some(ref ident) => ident.to_string() == "maxsize",
                        None => false,
                    })
            else {
                continue;
            };

            if maxsize.value.is_none_literal_expr() {
                let diagnostic = Diagnostic::new(MethodCacheMaxSizeNone, decorator.range());
                diagnostics.push(diagnostic);
            }
        }
    }
}
