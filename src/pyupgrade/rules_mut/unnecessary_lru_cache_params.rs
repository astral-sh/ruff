use rustpython_parser::ast::Expr;

use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::pyupgrade::rules;

/// UP011
pub fn unnecessary_lru_cache_params(checker: &mut Checker, decorator_list: &[Expr]) {
    let Some(mut diagnostic) = rules::unnecessary_lru_cache_params(
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
