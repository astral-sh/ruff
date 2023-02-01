use crate::ast::types::Range;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violations;
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::Location;

// Regex from PEP263.
static CODING_COMMENT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[ \t\f]*#.*?coding[:=][ \t]*utf-?8").unwrap());

/// UP009
pub fn unnecessary_coding_comment(lineno: usize, line: &str, autofix: bool) -> Option<Diagnostic> {
    // PEP3120 makes utf-8 the default encoding.
    if CODING_COMMENT_REGEX.is_match(line) {
        let mut diagnostic = Diagnostic::new(
            violations::PEP3120UnnecessaryCodingComment,
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
