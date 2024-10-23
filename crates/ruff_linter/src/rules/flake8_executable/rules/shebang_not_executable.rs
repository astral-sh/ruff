#![allow(unused_imports)]

use std::path::Path;

use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[cfg(target_family = "unix")]
use crate::rules::flake8_executable::helpers::is_executable;

/// ## What it does
/// Checks for a shebang directive in a file that is not executable.
///
/// ## Why is this bad?
/// In Python, a shebang (also known as a hashbang) is the first line of a
/// script, which specifies the interpreter that should be used to run the
/// script.
///
/// The presence of a shebang suggests that a file is intended to be
/// executable. If a file contains a shebang but is not executable, then the
/// shebang is misleading, or the file is missing the executable bit.
///
/// If the file is meant to be executable, add a shebang, as in:
/// ```python
/// #!/usr/bin/env python
/// ```
///
/// Otherwise, remove the executable bit from the file (e.g., `chmod -x __main__.py`).
///
/// A file is considered executable if it has the executable bit set (i.e., its
/// permissions mode intersects with `0o111`). As such, _this rule is only
/// available on Unix-like systems_, and is not enforced on Windows or WSL.
///
/// ## Example
/// ```python
/// #!/usr/bin/env python
/// ```
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
pub(crate) fn shebang_not_executable(filepath: &Path, range: TextRange) -> Option<Diagnostic> {
    // WSL supports Windows file systems, which do not have executable bits.
    // Instead, everything is executable. Therefore, we skip this rule on WSL.
    if is_wsl::is_wsl() {
        return None;
    }

    if let Ok(false) = is_executable(filepath) {
        return Some(Diagnostic::new(ShebangNotExecutable, range));
    }

    None
}

#[cfg(not(target_family = "unix"))]
pub(crate) fn shebang_not_executable(_filepath: &Path, _range: TextRange) -> Option<Diagnostic> {
    None
}
