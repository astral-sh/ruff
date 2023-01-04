use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::Location;

use crate::ast::types::Range;
use crate::registry::{Check, CheckKind};

static RST_BACKTICKS_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?!    ).*(^| )`[^`]+`([^_]|$)").unwrap());

/// PGH005 - Use two backticks when writing RST
pub fn rst_backticks(lineno: usize, line: &str) -> Option<Check> {
    RST_BACKTICKS_REGEX.find(line).map(|m| {
        Check::new(
            CheckKind::RstBackticks,
            Range::new(
                Location::new(lineno + 1, m.start()),
                Location::new(lineno + 1, m.end()),
            ),
        )
    })
}
