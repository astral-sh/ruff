use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Decorator, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for unnecessary parentheses on `functools.lru_cache` decorators.
///
/// ## Why is this bad?
/// Since Python 3.8, `functools.lru_cache` can be used as a decorator without
/// trailing parentheses, as long as no arguments are passed to it.
///
/// ## Example
/// ```python
/// import functools
///
///
/// @functools.lru_cache()
/// def foo():
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import functools
///
///
/// @functools.lru_cache
/// def foo():
///     ...
/// ```
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `@functools.lru_cache`](https://docs.python.org/3/library/functools.html#functools.lru_cache)
/// - [Let lru_cache be used as a decorator with no arguments](https://github.com/python/cpython/issues/80953)
#[violation]
pub struct LRUCacheWithoutParameters;

impl AlwaysAutofixableViolation for LRUCacheWithoutParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary parentheses to `functools.lru_cache`")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary parentheses".to_string()
    }
}

/// UP011
pub(crate) fn lru_cache_without_parameters(checker: &mut Checker, decorator_list: &[Decorator]) {
    for decorator in decorator_list.iter() {
        let Expr::Call(ast::ExprCall {
            func,
            args,
            keywords,
            range: _,
        }) = &decorator.expression
        else {
            continue;
        };

        // Look for, e.g., `import functools; @functools.lru_cache()`.
        if args.is_empty()
            && keywords.is_empty()
            && checker
                .semantic()
                .resolve_call_path(func)
                .map_or(false, |call_path| {
                    matches!(call_path.as_slice(), ["functools", "lru_cache"])
                })
        {
            let mut diagnostic = Diagnostic::new(
                LRUCacheWithoutParameters,
                TextRange::new(func.end(), decorator.end()),
            );
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    checker.generator().expr(func),
                    decorator.expression.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
