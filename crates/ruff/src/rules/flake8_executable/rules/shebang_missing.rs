#![allow(unused_imports)]

use std::path::Path;

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
#[cfg(target_family = "unix")]
use crate::rules::flake8_executable::helpers::is_executable;
use crate::violation::Violation;

define_violation!(
    pub struct ShebangMissingExecutableFile;
);
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
