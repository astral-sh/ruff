use rustpython_parser::ast::Expr;

use crate::check_ast::Checker;
use crate::pyupgrade::checks;

pub fn unnecessary_lru_cache_params(checker: &mut Checker, decorator_list: &[Expr]) {
    if let Some(check) = checks::unnecessary_lru_cache_params(
        decorator_list,
        checker.settings.target_version,
        checker.from_imports.get("functools"),
    ) {
        checker.add_check(check);
    }
}
