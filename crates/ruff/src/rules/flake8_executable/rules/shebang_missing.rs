#![allow(unused_imports)]

use std::path::Path;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::registry::AsRule;
#[cfg(target_family = "unix")]
use crate::rules::flake8_executable::helpers::is_executable;

#[violation]
pub struct ShebangMissingExecutableFile;

impl Violation for ShebangMissingExecutableFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The file is executable but no shebang is present")
    }
}

/// EXE002
#[cfg(target_family = "unix")]
pub fn shebang_missing(filepath: &Path) -> Option<Diagnostic> {
    if let Ok(true) = is_executable(filepath) {
        let diagnostic = Diagnostic::new(ShebangMissingExecutableFile, Range::default());
        return Some(diagnostic);
    }
    None
}

#[cfg(not(target_family = "unix"))]
pub fn shebang_missing(_filepath: &Path) -> Option<Diagnostic> {
    None
}
