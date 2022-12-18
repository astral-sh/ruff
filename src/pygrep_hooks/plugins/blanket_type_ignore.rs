use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::Location;

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

static BLANKET_TYPE_IGNORE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"# type:? *ignore($|\s)").unwrap());

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
