use rustpython_parser::ast::Location;

use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::registry::Diagnostic;
use crate::rules::flake8_executable::helpers::ShebangDirective;
use crate::violation::Violation;

#[violation]
pub struct ShebangPython;

impl Violation for ShebangPython {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Shebang should contain `python`")
    }
}

/// EXE003
pub fn shebang_python(lineno: usize, shebang: &ShebangDirective) -> Option<Diagnostic> {
    if let ShebangDirective::Match(_, start, end, content) = shebang {
        if content.contains("python") || content.contains("pytest") {
            None
        } else {
            let diagnostic = Diagnostic::new(
                ShebangPython,
                Range::new(
                    Location::new(lineno + 1, start - 2),
                    Location::new(lineno + 1, *end),
                ),
            );

            Some(diagnostic)
        }
    } else {
        None
    }
}
