//! Best-effort bulk metadata reads for regular files in a directory.

use std::ffi::OsString;
use std::path::Path;

use filetime::FileTime;
use rustc_hash::FxHashMap;

#[cfg(target_os = "macos")]
mod macos;

/// Metadata returned without following symbolic links.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileMetadata {
    pub last_modified: FileTime,
    /// Unix mode, including the regular-file type bits.
    pub mode: u32,
}

/// Reads regular-file metadata in batches when the platform supports it.
///
/// Results are a snapshot for one scan, not a persistent cache. Missing entries, including
/// symbolic links, must be queried individually. Errors and unsupported attributes return `None`
/// so callers retain the behavior of their ordinary metadata lookups.
pub fn read(directory: &Path) -> Option<FxHashMap<OsString, FileMetadata>> {
    #[cfg(target_os = "macos")]
    {
        match macos::read(directory) {
            Ok(entries) => entries,
            Err(error) => {
                tracing::debug!("Falling back to individual file metadata reads: {error}");
                None
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = directory;
        None
    }
}
