use rustpython_ast::{Constant, ExprKind, KeywordData};
use rustpython_parser::ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::settings::types::PythonVersion;
use crate::violations;

fn rule(
    checker: &Checker,
    decorator_list: &[Expr],
    target_version: PythonVersion,
) -> Option<Diagnostic> {
    for expr in decorator_list.iter() {
        let ExprKind::Call {
            func,
            args,
            keywords,
        } = &expr.node
        else {
            continue;
        };

        if !(args.is_empty()
            && checker
                .resolve_call_path(func)
                .map_or(false, |call_path| call_path == ["functools", "lru_cache"]))
        {
            continue;
        }

        let range = Range::new(func.end_location.unwrap(), expr.end_location.unwrap());
        // Ex) `functools.lru_cache()`
        if keywords.is_empty() {
            return Some(Diagnostic::new(
                violations::UnnecessaryLRUCacheParams,
                range,
            ));
        }
        // Ex) `functools.lru_cache(maxsize=None)`
        if !(target_version >= PythonVersion::Py39 && keywords.len() == 1) {
            continue;
        }

        let KeywordData { arg, value } = &keywords[0].node;
        if !(arg.as_ref().map(|arg| arg == "maxsize").unwrap_or_default()
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
        return Some(Diagnostic::new(
            violations::UnnecessaryLRUCacheParams,
            range,
        ));
    }
    None
}

/// UP011
pub fn unnecessary_lru_cache_params(checker: &mut Checker, decorator_list: &[Expr]) {
    let Some(mut diagnostic) = rule(
        checker,
        decorator_list,
        checker.settings.target_version,
    ) else {
        return;
    };
    if checker.patch(diagnostic.kind.code()) {
        diagnostic.amend(Fix::deletion(diagnostic.location, diagnostic.end_location));
    }
    checker.diagnostics.push(diagnostic);
}
