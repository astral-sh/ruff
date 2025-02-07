use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Decorator, Expr};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary parentheses on `functools.lru_cache` decorators.
///
/// ## Why is this bad?
/// Since Python 3.8, `functools.lru_cache` can be used as a decorator without
/// trailing parentheses, as long as no arguments are passed to it.
///
/// ## Example
///
/// ```python
/// import functools
///
///
/// @functools.lru_cache()
/// def foo(): ...
/// ```
///
/// Use instead:
///
/// ```python
/// import functools
///
///
/// @functools.lru_cache
/// def foo(): ...
/// ```
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `@functools.lru_cache`](https://docs.python.org/3/library/functools.html#functools.lru_cache)
/// - [Let lru_cache be used as a decorator with no arguments](https://github.com/python/cpython/issues/80953)
#[derive(ViolationMetadata)]
pub(crate) struct LRUCacheWithoutParameters;

impl AlwaysFixableViolation for LRUCacheWithoutParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary parentheses to `functools.lru_cache`".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove unnecessary parentheses".to_string()
    }
}

/// UP011
pub(crate) fn lru_cache_without_parameters(checker: &Checker, decorator_list: &[Decorator]) {
    for decorator in decorator_list {
        let Expr::Call(ast::ExprCall {
            func,
            arguments,
            range: _,
        }) = &decorator.expression
        else {
            continue;
        };

        // Look for, e.g., `import functools; @functools.lru_cache()`.
        if arguments.args.is_empty()
            && arguments.keywords.is_empty()
            && checker
                .semantic()
                .resolve_qualified_name(func)
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["functools", "lru_cache"])
                })
        {
            let mut diagnostic = Diagnostic::new(
                LRUCacheWithoutParameters,
                TextRange::new(func.end(), decorator.end()),
            );
            diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(arguments.range())));
            checker.report_diagnostic(diagnostic);
        }
    }
}
