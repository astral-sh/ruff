use rustpython_parser::ast::Location;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::rules::flake8_executable::helpers::ShebangDirective;

#[violation]
pub struct ShebangNotFirstLine;

impl Violation for ShebangNotFirstLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Shebang should be at the beginning of the file")
    }
}

/// EXE005
pub fn shebang_newline(lineno: usize, shebang: &ShebangDirective) -> Option<Diagnostic> {
    if let ShebangDirective::Match(_, start, end, _) = shebang {
        if lineno > 1 {
            let diagnostic = Diagnostic::new(
                ShebangNotFirstLine,
                Range::new(
                    Location::new(lineno + 1, *start),
                    Location::new(lineno + 1, *end),
                ),
            );
            Some(diagnostic)
        } else {
            None
        }
    } else {
        None
    }
}
