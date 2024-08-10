use ruff_python_ast::{self as ast, Arguments, Decorator, Expr, Keyword};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for uses of `functools.lru_cache` that set `maxsize=None`.
///
/// ## Why is this bad?
/// Since Python 3.9, `functools.cache` can be used as a drop-in replacement
/// for `functools.lru_cache(maxsize=None)`. When possible, prefer
/// `functools.cache` as it is more readable and idiomatic.
///
/// ## Example
///
/// ```python
/// import functools
///
///
/// @functools.lru_cache(maxsize=None)
/// def foo(): ...
/// ```
///
/// Use instead:
///
/// ```python
/// import functools
///
///
/// @functools.cache
/// def foo(): ...
/// ```
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `@functools.cache`](https://docs.python.org/3/library/functools.html#functools.cache)
#[violation]
pub struct LRUCacheWithMaxsizeNone;

impl AlwaysFixableViolation for LRUCacheWithMaxsizeNone {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `@functools.cache` instead of `@functools.lru_cache(maxsize=None)`")
    }

    fn fix_title(&self) -> String {
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
                .resolve_qualified_name(func)
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["functools", "lru_cache"])
                })
        {
            let Keyword {
                arg,
                value,
                range: _,
            } = &keywords[0];
            if arg.as_ref().is_some_and(|arg| arg == "maxsize") && value.is_none_literal_expr() {
                let mut diagnostic = Diagnostic::new(
                    LRUCacheWithMaxsizeNone,
                    TextRange::new(func.end(), decorator.end()),
                );
                diagnostic.try_set_fix(|| {
                    let (import_edit, binding) = checker.importer().get_or_import_symbol(
                        &ImportRequest::import("functools", "cache"),
                        decorator.start(),
                        checker.semantic(),
                    )?;
                    let reference_edit =
                        Edit::range_replacement(binding, decorator.expression.range());
                    Ok(Fix::safe_edits(import_edit, [reference_edit]))
                });
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
