#![allow(unused_imports)]

use std::path::Path;

use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

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
/// If the file is meant to be executable, add the executable bit to the file
/// (e.g., `chmod +x __main__.py` or `git update-index --chmod=+x __main__.py`).
///
/// Otherwise, remove the shebang.
///
/// ## Filesystem considerations
///
/// A file is considered executable if it has the executable bit set (i.e., its
/// permissions mode intersects with `0o111`). As such, _this rule is only
/// available on Unix-like filesystems_.
///
/// It is not enforced on Windows, and will never trigger on Unix-like systems
/// if the _project root_ is located on a _filesystem which does not support
/// Unix-like permissions_ (e.g. mounting an removable drive using FAT or using
/// /mnt/c/ on WSL).
///
/// ## Example
/// ```python
/// #!/usr/bin/env python
/// ```
///
/// ## References
/// - [Python documentation: Executable Python Scripts](https://docs.python.org/3/tutorial/appendix.html#executable-python-scripts)
/// - [Git documentation: `git update-index --chmod`](https://git-scm.com/docs/git-update-index#Documentation/git-update-index.txt---chmod-x)
/// - [WSL documentation: Working across filesystems](https://learn.microsoft.com/en-us/windows/wsl/filesystems)
#[derive(ViolationMetadata)]
pub(crate) struct ShebangNotExecutable;

impl Violation for ShebangNotExecutable {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Shebang is present but file is not executable".to_string()
    }
}

/// EXE001
#[cfg(target_family = "unix")]
pub(crate) fn shebang_not_executable(filepath: &Path, range: TextRange) -> Option<Diagnostic> {
    if let Ok(false) = is_executable(filepath) {
        return Some(Diagnostic::new(ShebangNotExecutable, range));
    }
    None
}

#[cfg(not(target_family = "unix"))]
pub(crate) fn shebang_not_executable(_filepath: &Path, _range: TextRange) -> Option<Diagnostic> {
    None
}
