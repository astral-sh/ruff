#![cfg(target_family = "unix")]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::OnceLock;

use anyhow::Result;

pub(super) fn is_executable(filepath: &Path) -> Result<bool> {
    let metadata = filepath.metadata()?;
    let permissions = metadata.permissions();
    Ok(permissions.mode() & 0o111 != 0)
}

/// Returns `true` if the current process is running under WSL.
pub(super) fn is_wsl() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(detect_wsl)
}

fn detect_wsl() -> bool {
    if std::env::consts::OS != "linux" {
        return false;
    }

    let has_microsoft_kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .or_else(|_| std::fs::read_to_string("/proc/version"))
        .is_ok_and(|s| s.to_ascii_lowercase().contains("microsoft"));

    if !has_microsoft_kernel {
        return false;
    }

    // A container on a WSL2 host still has "microsoft" in the kernel version
    // string, but the filesystem has proper Unix semantics, so we should not
    // treat it as WSL.
    !is_container()
}

/// Detects whether the current process is running inside a container.
///
/// Checks for Docker (`/.dockerenv`), Podman (`/run/.containerenv`), and
/// various container runtimes via their cgroup paths.
fn is_container() -> bool {
    if std::fs::metadata("/.dockerenv").is_ok() || std::fs::metadata("/run/.containerenv").is_ok() {
        return true;
    }

    std::fs::read_to_string("/proc/self/cgroup").is_ok_and(|cgroup| {
        let cgroup = cgroup.to_ascii_lowercase();
        cgroup.contains("docker")
            || cgroup.contains("/container")
            || cgroup.contains("/lxc")
            || cgroup.contains("kubepods")
    })
}
