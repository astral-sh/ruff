use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::Location;

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

static BLANKET_NOQA_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)# noqa($|\s|:[^ ])").unwrap());

/// PGH004 - use of blanket noqa comments
pub fn blanket_noqa(lineno: usize, line: &str) -> Option<Diagnostic> {
    BLANKET_NOQA_REGEX.find(line).map(|m| {
        Diagnostic::new(
            violations::BlanketNOQA,
            Range::new(
                Location::new(lineno + 1, m.start()),
                Location::new(lineno + 1, m.end()),
            ),
        )
    })
}
