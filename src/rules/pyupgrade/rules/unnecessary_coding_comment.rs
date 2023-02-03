use crate::ast::types::Range;
use crate::define_simple_autofix_violation;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

use once_cell::sync::Lazy;
use regex::Regex;
use ruff_macros::derive_message_formats;
use rustpython_ast::Location;

define_simple_autofix_violation!(
    PEP3120UnnecessaryCodingComment,
    "UTF-8 encoding declaration is unnecessary",
    "Remove unnecessary coding comment"
);

// Regex from PEP263.
static CODING_COMMENT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[ \t\f]*#.*?coding[:=][ \t]*utf-?8").unwrap());

/// UP009
pub fn unnecessary_coding_comment(lineno: usize, line: &str, autofix: bool) -> Option<Diagnostic> {
    // PEP3120 makes utf-8 the default encoding.
    if CODING_COMMENT_REGEX.is_match(line) {
        let mut diagnostic = Diagnostic::new(
            PEP3120UnnecessaryCodingComment,
            Range::new(Location::new(lineno + 1, 0), Location::new(lineno + 2, 0)),
        );
        if autofix {
            diagnostic.amend(Fix::deletion(
                Location::new(lineno + 1, 0),
                Location::new(lineno + 2, 0),
            ));
        }
        Some(diagnostic)
    } else {
        None
    }
}
