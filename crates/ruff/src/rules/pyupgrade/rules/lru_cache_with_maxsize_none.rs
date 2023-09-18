use ruff_python_ast::{self as ast, Arguments, Decorator, Expr, Keyword};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of `functools.lru_cache` that set `maxsize=None`.
///
/// ## Why is this bad?
/// Since Python 3.9, `functools.cache` can be used as a drop-in replacement
/// for `functools.lru_cache(maxsize=None)`. When possible, prefer
/// `functools.cache` as it is more readable and idiomatic.
///
/// ## Example
/// ```python
/// import functools
///
///
/// @functools.lru_cache(maxsize=None)
/// def foo():
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import functools
///
///
/// @functools.cache
/// def foo():
///     ...
/// ```
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `@functools.cache`](https://docs.python.org/3/library/functools.html#functools.cache)
#[violation]
pub struct LRUCacheWithMaxsizeNone;

impl AlwaysAutofixableViolation for LRUCacheWithMaxsizeNone {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `@functools.cache` instead of `@functools.lru_cache(maxsize=None)`")
    }

    fn autofix_title(&self) -> String {
        "Rewrite with `@functools.cache".to_string()
    }
}

/// UP033
pub(crate) fn lru_cache_with_maxsize_none(checker: &mut Checker, decorator_list: &[Decorator]) {
    for decorator in decorator_list {
        let Expr::Call(ast::ExprCall {
            func,
            arguments:
                Arguments {
                    args,
                    keywords,
                    range: _,
                },
            range: _,
        }) = &decorator.expression
        else {
            continue;
        };

        // Look for, e.g., `import functools; @functools.lru_cache(maxsize=None)`.
        if args.is_empty()
            && keywords.len() == 1
            && checker
                .semantic()
                .resolve_call_path(func)
                .is_some_and(|call_path| matches!(call_path.as_slice(), ["functools", "lru_cache"]))
        {
            let Keyword {
                arg,
                value,
                range: _,
            } = &keywords[0];
            if arg.as_ref().is_some_and(|arg| arg == "maxsize") && is_const_none(value) {
                let mut diagnostic = Diagnostic::new(
                    LRUCacheWithMaxsizeNone,
                    TextRange::new(func.end(), decorator.end()),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.try_set_fix(|| {
                        let (import_edit, binding) = checker.importer().get_or_import_symbol(
                            &ImportRequest::import("functools", "cache"),
                            decorator.start(),
                            checker.semantic(),
                        )?;
                        let reference_edit =
                            Edit::range_replacement(binding, decorator.expression.range());
                        Ok(Fix::automatic_edits(import_edit, [reference_edit]))
                    });
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
