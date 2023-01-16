use rustpython_ast::{Constant, ExprKind, KeywordData};
use rustpython_parser::ast::Expr;

use crate::ast::helpers::{create_expr, unparse_expr};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, RuleCode};
use crate::settings::types::PythonVersion;
use crate::violations;

/// UP011
pub fn unnecessary_lru_cache_params(checker: &mut Checker, decorator_list: &[Expr]) {
    for expr in decorator_list.iter() {
        let ExprKind::Call {
            func,
            args,
            keywords,
        } = &expr.node else {
            continue;
        };

        // Look for, e.g., `import functools; @functools.lru_cache`.
        if !(args.is_empty()
            && checker
                .resolve_call_path(func)
                .map_or(false, |call_path| call_path == ["functools", "lru_cache"]))
        {
            continue;
        }

        // Ex) `functools.lru_cache()`
        if keywords.is_empty() {
            let mut diagnostic = Diagnostic::new(
                violations::UnnecessaryLRUCacheParams,
                Range::new(func.end_location.unwrap(), expr.end_location.unwrap()),
            );
            if checker.patch(&RuleCode::UP011) {
                diagnostic.amend(Fix::replacement(
                    unparse_expr(func, checker.stylist),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }

        // Ex) `functools.lru_cache(maxsize=None)`
        if !(checker.settings.target_version >= PythonVersion::Py39 && keywords.len() == 1) {
            continue;
        }

        let KeywordData { arg, value } = &keywords[0].node;
        if !(arg.as_ref().map_or(false, |arg| arg == "maxsize")
            && matches!(
                value.node,
                ExprKind::Constant {
                    value: Constant::None,
                    kind: None,
                }
            ))
        {
            continue;
        }

        let mut diagnostic = Diagnostic::new(
            violations::UnnecessaryLRUCacheParams,
            Range::new(func.end_location.unwrap(), expr.end_location.unwrap()),
        );
        if checker.patch(&RuleCode::UP011) {
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
