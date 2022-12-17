use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::{Expr, Location};

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

static BLANKET_TYPE_IGNORE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"# type:? *ignore($|\s)").unwrap());

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

/// PGH003 - use of blanket type ignore comments
pub fn blanket_type_ignore(lineno: usize, line: &str) -> Option<Check> {
    BLANKET_TYPE_IGNORE_REGEX.find(line).map(|m| {
        Check::new(
            CheckKind::BlanketTypeIgnore,
            Range {
                location: Location::new(lineno + 1, m.start()),
                end_location: Location::new(lineno + 1, m.end()),
            },
        )
    })
}
