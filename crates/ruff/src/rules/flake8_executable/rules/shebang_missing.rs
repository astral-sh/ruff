use std::path::Path;

#[cfg(not(target_family = "wasm"))]
use is_executable::IsExecutable;
use ruff_macros::{define_violation, derive_message_formats};

#[cfg(not(target_family = "wasm"))]
use crate::ast::types::Range;
use crate::registry::Diagnostic;
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
#[cfg(not(target_family = "wasm"))]
pub fn shebang_missing(filepath: &Path) -> Option<Diagnostic> {
    if filepath.is_executable() {
        let diagnostic = Diagnostic::new(ShebangMissingExecutableFile, Range::default());
        Some(diagnostic)
    } else {
        None
    }
}

#[cfg(target_family = "wasm")]
pub fn shebang_missing(_filepath: &Path) -> Option<Diagnostic> {
    None
}
