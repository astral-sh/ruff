#[cfg(not(target_family = "wasm"))]
use std::os::unix::prelude::MetadataExt;
use std::path::Path;

use ruff_macros::derive_message_formats;

#[cfg(not(target_family = "wasm"))]
use crate::ast::types::Range;
use crate::define_violation;
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
    if let Ok(metadata) = filepath.metadata() {
        // Check if file is executable by anyone
        if metadata.mode() & 0o111 == 0 {
            None
        } else {
            let diagnostic = Diagnostic::new(ShebangMissingExecutableFile, Range::default());
            Some(diagnostic)
        }
    } else {
        None
    }
}

#[cfg(target_family = "wasm")]
pub fn shebang_missing(_filepath: &Path) -> Option<Diagnostic> {
    None
}
