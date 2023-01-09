use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Constant, ExprKind, KeywordData};
use rustpython_parser::ast::Expr;

use crate::ast::helpers;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::settings::types::PythonVersion;
use crate::violations;

fn rule(
    decorator_list: &[Expr],
    target_version: PythonVersion,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
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
            && helpers::match_module_member(
                func,
                "functools",
                "lru_cache",
                from_imports,
                import_aliases,
            ))
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
        decorator_list,
        checker.settings.target_version,
        &checker.from_imports,
        &checker.import_aliases,
    ) else {
        return;
    };
    if checker.patch(diagnostic.kind.code()) {
        diagnostic.amend(Fix::deletion(diagnostic.location, diagnostic.end_location));
    }
    checker.diagnostics.push(diagnostic);
}
