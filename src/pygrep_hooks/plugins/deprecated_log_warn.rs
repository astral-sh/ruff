use rustpython_ast::Expr;

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// PGH002 - deprecated use of logging.warn
pub fn deprecated_log_warn(checker: &mut Checker, func: &Expr) {
    let call_path = dealias_call_path(collect_call_paths(func), &checker.import_aliases);
    if call_path == ["log", "warn"]
        || match_call_path(&call_path, "logging", "warn", &checker.from_imports)
    {
        checker.add_check(Check::new(
            CheckKind::DeprecatedLogWarn,
            Range::from_located(func),
        ));
    }
}
