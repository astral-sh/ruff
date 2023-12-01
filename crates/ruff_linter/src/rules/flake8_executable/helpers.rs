#![cfg(target_family = "unix")]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::Result;

pub(super) fn is_executable(filepath: &Path) -> Result<bool> {
    let metadata = filepath.metadata()?;
    let permissions = metadata.permissions();
    Ok(permissions.mode() & 0o111 != 0)
}
