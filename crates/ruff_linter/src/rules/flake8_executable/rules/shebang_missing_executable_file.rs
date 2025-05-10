#![allow(unused_imports)]

use std::path::Path;

use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

#[cfg(target_family = "unix")]
use crate::rules::flake8_executable::helpers::{executable_by_default, is_executable};
use crate::settings::LinterSettings;

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
/// ## Filesystem considerations
///
/// A file is considered executable if it has the executable bit set (i.e., its
/// permissions mode intersects with `0o111`). As such, _this rule is only
/// available on Unix-like filesystems_.
///
/// It is not enforced on Windows, nor on Unix-like systems if the _project root_
/// is located on a _filesystem which does not support Unix-like permissions_
/// (e.g. mounting an removable drive using FAT or using /mnt/c/ on WSL).
///
/// ## References
/// - [Python documentation: Executable Python Scripts](https://docs.python.org/3/tutorial/appendix.html#executable-python-scripts)
/// - [Git documentation: `git update-index --chmod`](https://git-scm.com/docs/git-update-index#Documentation/git-update-index.txt---chmod-x)
/// - [WSL documentation: Working across filesystems](https://learn.microsoft.com/en-us/windows/wsl/filesystems)
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
pub(crate) fn shebang_missing_executable_file(
    filepath: &Path,
    settings: &LinterSettings,
) -> Option<Diagnostic> {
    if let Ok(true) = is_executable(filepath) {
        if !executable_by_default(settings) {
            return Some(Diagnostic::new(
                ShebangMissingExecutableFile,
                TextRange::default(),
            ));
        }
    }
    None
}

#[cfg(not(target_family = "unix"))]
pub(crate) fn shebang_missing_executable_file(
    _filepath: &Path,
    _settings: &LinterSettings,
) -> Option<Diagnostic> {
    None
}
