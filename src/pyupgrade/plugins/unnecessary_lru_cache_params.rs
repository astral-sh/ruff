use rustpython_parser::ast::Expr;

use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::pyupgrade::checks;

/// UP011
pub fn unnecessary_lru_cache_params(checker: &mut Checker, decorator_list: &[Expr]) {
    let Some(mut check) = checks::unnecessary_lru_cache_params(
        decorator_list,
        checker.settings.target_version,
        &checker.from_imports,
        &checker.import_aliases,
    ) else {
        return;
    };
    if checker.patch(check.kind.code()) {
        check.amend(Fix::deletion(check.location, check.end_location));
    }
    checker.add_check(check);
}
