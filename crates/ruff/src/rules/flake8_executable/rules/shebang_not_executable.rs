#![allow(unused_imports)]

use std::path::Path;

use rustpython_parser::ast::Location;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::registry::AsRule;
#[cfg(target_family = "unix")]
use crate::rules::flake8_executable::helpers::is_executable;
use crate::rules::flake8_executable::helpers::ShebangDirective;

#[violation]
pub struct ShebangNotExecutable;

impl Violation for ShebangNotExecutable {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Shebang is present but file is not executable")
    }
}

/// EXE001
#[cfg(target_family = "unix")]
pub fn shebang_not_executable(
    filepath: &Path,
    lineno: usize,
    shebang: &ShebangDirective,
) -> Option<Diagnostic> {
    if let ShebangDirective::Match(_, start, end, _) = shebang {
        if let Ok(false) = is_executable(filepath) {
            let diagnostic = Diagnostic::new(
                ShebangNotExecutable,
                Range::new(
                    Location::new(lineno + 1, *start),
                    Location::new(lineno + 1, *end),
                ),
            );
            return Some(diagnostic);
        }
    }
    None
}

#[cfg(not(target_family = "unix"))]
pub fn shebang_not_executable(
    _filepath: &Path,
    _lineno: usize,
    _shebang: &ShebangDirective,
) -> Option<Diagnostic> {
    None
}
