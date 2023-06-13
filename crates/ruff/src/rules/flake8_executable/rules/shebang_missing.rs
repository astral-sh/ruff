#![allow(unused_imports)]

use std::path::Path;

use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::registry::AsRule;
#[cfg(target_family = "unix")]
use crate::rules::flake8_executable::helpers::is_executable;

/// ## What it does
/// Checks for executable files that do not have a shebang directive.
///
/// ## Why is this bad?
/// Shebangs indicate that a file is executable. If the file has executable
/// permissions but no shebang, then the absence of a shebang is potentially
/// misleading and is likely a mistake.
///
/// Instead, add a shebang or remove the executable permissions with
/// `chmod -x`.
///
/// _This rule is only available on Unix-like systems._
///
/// ## References
/// - [Python documentation: Executable Python Scripts](https://docs.python.org/3/tutorial/appendix.html#executable-python-scripts)
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
pub(crate) fn shebang_missing(filepath: &Path) -> Option<Diagnostic> {
    if let Ok(true) = is_executable(filepath) {
        let diagnostic = Diagnostic::new(ShebangMissingExecutableFile, TextRange::default());
        return Some(diagnostic);
    }
    None
}

#[cfg(not(target_family = "unix"))]
pub(crate) fn shebang_missing(_filepath: &Path) -> Option<Diagnostic> {
    None
}
