#![cfg(target_family = "unix")]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::OnceLock;

use anyhow::Result;
use tempfile::NamedTempFile;

use crate::settings::LinterSettings;

pub(super) fn is_executable(filepath: &Path) -> Result<bool> {
    let metadata = filepath.metadata()?;
    let permissions = metadata.permissions();
    Ok(permissions.mode() & 0o111 != 0)
}

// Some file systems do not support executable bits. Instead, everything is executable.
// See #3110, #5445, #10084, #12941
//
// Benchmarking shows better performance vs previous, incorrect approach of checking `is_wsl()`
// as long as we use a `OnceLock` and a simple test first (filemode of pyproject.toml).
static EXECUTABLE_BY_DEFAULT: OnceLock<bool> = OnceLock::new();

pub(super) fn executable_by_default(settings: &LinterSettings) -> bool {
    *EXECUTABLE_BY_DEFAULT.get_or_init(|| {
        is_executable(&settings.project_root.join("pyproject.toml")).unwrap_or(true)
            // if pyproject.toml is executable or doesn't exist, run a slower check too:
            && NamedTempFile::new_in(&settings.project_root)
                .map_err(std::convert::Into::into)
                .and_then(|tmpfile| is_executable(tmpfile.path()))
                .unwrap_or(false) // Assume a normal filesystem in case of read-only, IOError, ...
    })
}
