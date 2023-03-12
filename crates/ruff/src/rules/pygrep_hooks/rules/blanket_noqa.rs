use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::Location;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

#[violation]
pub struct BlanketNOQA;

impl Violation for BlanketNOQA {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use specific rule codes when using `noqa`")
    }
}

static BLANKET_NOQA_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)# noqa($|\s|:[^ ])").unwrap());

/// PGH004 - use of blanket noqa comments
pub fn blanket_noqa(lineno: usize, line: &str) -> Option<Diagnostic> {
    BLANKET_NOQA_REGEX.find(line).map(|m| {
        Diagnostic::new(
            BlanketNOQA,
            Range::new(
                Location::new(lineno + 1, m.start()),
                Location::new(lineno + 1, m.end()),
            ),
        )
    })
}
