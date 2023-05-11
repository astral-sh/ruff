use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr, ExprKind, KeywordData};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::autofix::actions::get_or_import_symbol;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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
pub(crate) fn lru_cache_with_maxsize_none(checker: &mut Checker, decorator_list: &[Expr]) {
    for expr in decorator_list.iter() {
        let ExprKind::Call(ast::ExprCall {
            func,
            args,
            keywords,
        }) = &expr.node else {
            continue;
        };

        // Look for, e.g., `import functools; @functools.lru_cache(maxsize=None)`.
        if args.is_empty()
            && keywords.len() == 1
            && checker
                .ctx
                .resolve_call_path(func)
                .map_or(false, |call_path| {
                    call_path.as_slice() == ["functools", "lru_cache"]
                })
        {
            let KeywordData { arg, value } = &keywords[0].node;
            if arg.as_ref().map_or(false, |arg| arg == "maxsize")
                && matches!(
                    value.node,
                    ExprKind::Constant(ast::ExprConstant {
                        value: Constant::None,
                        kind: None,
                    })
                )
            {
                let mut diagnostic = Diagnostic::new(
                    LRUCacheWithMaxsizeNone,
                    TextRange::new(func.end(), expr.end()),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.try_set_fix(|| {
                        let (import_edit, binding) = get_or_import_symbol(
                            "functools",
                            "cache",
                            expr.start(),
                            &checker.ctx,
                            &checker.importer,
                            checker.locator,
                        )?;
                        let reference_edit = Edit::range_replacement(binding, expr.range());
                        #[allow(deprecated)]
                        Ok(Fix::unspecified_edits(import_edit, [reference_edit]))
                    });
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
