#![allow(unused_imports)]

use std::path::Path;

use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::registry::AsRule;
#[cfg(target_family = "unix")]
use crate::rules::flake8_executable::helpers::is_executable;
use crate::rules::flake8_executable::helpers::ShebangDirective;

/// ## What it does
/// Checks for a shebang directive in a file that is not executable.
///
/// ## Why is this bad?
/// Shebangs indicate that a file is executable. If the file is not executable,
/// then the shebang is misleading and is likely a mistake.
///
/// Instead, remove the shebang or give the file executable permissions with
/// `chmod +x`.
///
/// _This rule is only available on Unix-like systems._
///
/// ## References
/// - [Python documentation: Executable Python Scripts](https://docs.python.org/3/tutorial/appendix.html#executable-python-scripts)
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
pub(crate) fn shebang_not_executable(
    filepath: &Path,
    range: TextRange,
    shebang: &ShebangDirective,
) -> Option<Diagnostic> {
    if let ShebangDirective::Match(_, start, content) = shebang {
        if let Ok(false) = is_executable(filepath) {
            let diagnostic = Diagnostic::new(
                ShebangNotExecutable,
                TextRange::at(range.start() + start, content.text_len()),
            );
            return Some(diagnostic);
        }
    }
    None
}

#[cfg(not(target_family = "unix"))]
pub(crate) fn shebang_not_executable(
    _filepath: &Path,
    _range: TextRange,
    _shebang: &ShebangDirective,
) -> Option<Diagnostic> {
    None
}
