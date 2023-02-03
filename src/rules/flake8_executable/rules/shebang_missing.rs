use std::path::Path;

#[cfg(not(target_family = "wasm"))]
use is_executable::IsExecutable;
use ruff_macros::derive_message_formats;

#[cfg(not(target_family = "wasm"))]
use crate::ast::types::Range;
use crate::define_simple_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_simple_violation!(
    ShebangMissingExecutableFile,
    "The file is executable but no shebang is present"
);

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
