#![cfg(target_family = "unix")]

use anyhow::Result;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub(super) fn is_executable(filepath: &Path) -> Result<bool> {
    let metadata = filepath.metadata()?;
    let permissions = metadata.permissions();
    Ok(permissions.mode() & 0o111 != 0)
}
