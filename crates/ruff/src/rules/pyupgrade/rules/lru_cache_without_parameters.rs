use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_expr;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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
pub(crate) fn lru_cache_without_parameters(checker: &mut Checker, decorator_list: &[Expr]) {
    for expr in decorator_list.iter() {
        let ExprKind::Call(ast::ExprCall {
            func,
            args,
            keywords,
        }) = &expr.node else {
            continue;
        };

        // Look for, e.g., `import functools; @functools.lru_cache()`.
        if args.is_empty()
            && keywords.is_empty()
            && checker
                .ctx
                .resolve_call_path(func)
                .map_or(false, |call_path| {
                    call_path.as_slice() == ["functools", "lru_cache"]
                })
        {
            let mut diagnostic = Diagnostic::new(
                LRUCacheWithoutParameters,
                TextRange::new(func.end(), expr.end()),
            );
            if checker.patch(diagnostic.kind.rule()) {
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                    unparse_expr(func, checker.stylist),
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
