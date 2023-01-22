use rustpython_ast::ExprKind;
use rustpython_parser::ast::Expr;

use crate::ast::helpers::unparse_expr;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violations;

/// UP011
pub fn lru_cache_without_parameters(checker: &mut Checker, decorator_list: &[Expr]) {
    for expr in decorator_list.iter() {
        let ExprKind::Call {
            func,
            args,
            keywords,
        } = &expr.node else {
            continue;
        };

        // Look for, e.g., `import functools; @functools.lru_cache()`.
        if args.is_empty()
            && keywords.is_empty()
            && checker.resolve_call_path(func).map_or(false, |call_path| {
                call_path.as_slice() == ["functools", "lru_cache"]
            })
        {
            let mut diagnostic = Diagnostic::new(
                violations::LRUCacheWithoutParameters,
                Range::new(func.end_location.unwrap(), expr.end_location.unwrap()),
            );
            if checker.patch(&Rule::LRUCacheWithoutParameters) {
                diagnostic.amend(Fix::replacement(
                    unparse_expr(func, checker.stylist),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
