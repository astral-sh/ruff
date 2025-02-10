#![allow(unused_imports)]

use std::path::Path;

use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use crate::registry::AsRule;
#[cfg(target_family = "unix")]
use crate::rules::flake8_executable::helpers::is_executable;

/// ## What it does
/// Checks for executable `.py` files that do not have a shebang.
///
/// ## Why is this bad?
/// In Python, a shebang (also known as a hashbang) is the first line of a
/// script, which specifies the interpreter that should be used to run the
/// script.
///
/// If a `.py` file is executable, but does not have a shebang, it may be run
/// with the wrong interpreter, or fail to run at all.
///
/// If the file is meant to be executable, add a shebang, as in:
/// ```python
/// #!/usr/bin/env python
/// ```
///
/// Otherwise, remove the executable bit from the file
/// (e.g., `chmod -x __main__.py` or `git update-index --chmod=-x __main__.py`).
///
/// A file is considered executable if it has the executable bit set (i.e., its
/// permissions mode intersects with `0o111`). As such, _this rule is only
/// available on Unix-like systems_, and is not enforced on Windows or WSL.
///
/// ## References
/// - [Python documentation: Executable Python Scripts](https://docs.python.org/3/tutorial/appendix.html#executable-python-scripts)
/// - [Git documentation: `git update-index --chmod`](https://git-scm.com/docs/git-update-index#Documentation/git-update-index.txt---chmod-x)
#[derive(ViolationMetadata)]
pub(crate) struct ShebangMissingExecutableFile;

impl Violation for ShebangMissingExecutableFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        "The file is executable but no shebang is present".to_string()
    }
}

/// EXE002
#[cfg(target_family = "unix")]
pub(crate) fn shebang_missing_executable_file(filepath: &Path) -> Option<Diagnostic> {
    // WSL supports Windows file systems, which do not have executable bits.
    // Instead, everything is executable. Therefore, we skip this rule on WSL.
    if is_wsl::is_wsl() {
        return None;
    }
    if let Ok(true) = is_executable(filepath) {
        return Some(Diagnostic::new(
            ShebangMissingExecutableFile,
            TextRange::default(),
        ));
    }
    None
}

#[cfg(not(target_family = "unix"))]
pub(crate) fn shebang_missing_executable_file(_filepath: &Path) -> Option<Diagnostic> {
    None
}
