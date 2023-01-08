use rustpython_parser::ast::Expr;

use crate::autofix::Fix;
use crate::pyupgrade::checks;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// UP011
pub fn unnecessary_lru_cache_params(xxxxxxxx: &mut xxxxxxxx, decorator_list: &[Expr]) {
    let Some(mut check) = checks::unnecessary_lru_cache_params(
        decorator_list,
        xxxxxxxx.settings.target_version,
        &xxxxxxxx.from_imports,
        &xxxxxxxx.import_aliases,
    ) else {
        return;
    };
    if xxxxxxxx.patch(check.kind.code()) {
        check.amend(Fix::deletion(check.location, check.end_location));
    }
    xxxxxxxx.diagnostics.push(check);
}
