use rustpython_parser::ast::Expr;

use crate::check_ast::Checker;
use crate::pyupgrade::{checks, fixes};

/// U011
pub fn unnecessary_lru_cache_params(checker: &mut Checker, decorator_list: &[Expr]) {
    if let Some(mut check) = checks::unnecessary_lru_cache_params(
        decorator_list,
        checker.settings.target_version,
        checker.from_imports.get("functools"),
    ) {
        if checker.patch() {
            if let Some(fix) =
                fixes::remove_unnecessary_lru_cache_params(checker.locator, &check.location)
            {
                check.amend(fix);
            }
        }
        checker.add_check(check);
    }
}
