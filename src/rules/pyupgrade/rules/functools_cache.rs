use rustpython_ast::{Constant, ExprKind, KeywordData};
use rustpython_parser::ast::Expr;

use crate::ast::helpers::{create_expr, unparse_expr};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violations;

/// UP033
pub fn functools_cache(checker: &mut Checker, decorator_list: &[Expr]) {
    for expr in decorator_list.iter() {
        let ExprKind::Call {
            func,
            args,
            keywords,
        } = &expr.node else {
            continue;
        };

        // Look for, e.g., `import functools; @functools.lru_cache(maxsize=None)`.
        if args.is_empty()
            && keywords.len() == 1
            && checker.resolve_call_path(func).map_or(false, |call_path| {
                call_path.as_slice() == ["functools", "lru_cache"]
            })
        {
            let KeywordData { arg, value } = &keywords[0].node;
            if arg.as_ref().map_or(false, |arg| arg == "maxsize")
                && matches!(
                    value.node,
                    ExprKind::Constant {
                        value: Constant::None,
                        kind: None,
                    }
                )
            {
                let mut diagnostic = Diagnostic::new(
                    violations::FunctoolsCache,
                    Range::new(func.end_location.unwrap(), expr.end_location.unwrap()),
                );
                if checker.patch(&Rule::FunctoolsCache) {
                    if let ExprKind::Attribute { value, ctx, .. } = &func.node {
                        diagnostic.amend(Fix::replacement(
                            unparse_expr(
                                &create_expr(ExprKind::Attribute {
                                    value: value.clone(),
                                    attr: "cache".to_string(),
                                    ctx: ctx.clone(),
                                }),
                                checker.stylist,
                            ),
                            expr.location,
                            expr.end_location.unwrap(),
                        ));
                    }
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
