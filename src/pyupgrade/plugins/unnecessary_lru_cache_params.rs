use rustpython_parser::ast::Expr;

use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::pyupgrade::checks;

/// U011
pub fn unnecessary_lru_cache_params(checker: &mut Checker, decorator_list: &[Expr]) {
    if let Some(mut check) = checks::unnecessary_lru_cache_params(
        decorator_list,
        checker.settings.target_version,
        &checker.from_imports,
        &checker.import_aliases,
    ) {
        if checker.patch(check.kind.code()) {
            check.amend(Fix::deletion(check.location, check.end_location));
        }
        checker.add_check(check);
    }
}
