//! On-disk cache for ty's vendored typeshed stubs.
//!
//! ty embeds typeshed stubs in the binary via `ty_vendored`. These stubs live in a
//! `VendoredFileSystem` (an in-memory zip) and are not accessible from the host filesystem.
//! Pylance needs real filesystem paths so that its binder can resolve declarations
//! (e.g., `builtins.pyi`) instead of falling back to synthesized stubs.
//!
//! This module extracts the embedded typeshed to a temporary directory at server startup
//! and provides the extracted path for use in TSP search-path responses.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ruff_db::vendored::{FileType, VendoredFileSystem, VendoredPath};

/// Global cache holding the extracted typeshed path.
///
/// Once extracted, the path is valid for the lifetime of the process.
/// The temp directory is **not** cleaned up on drop — it will be cleaned
/// up by the OS's temp-directory reaper. This avoids the complexity of
/// tracking ownership across threads while the server is running.
static EXTRACTED_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Get the base path of the extracted typeshed directory on disk.
///
/// On the first call, this extracts all vendored typeshed files from the
/// in-memory zip into a temporary directory. Subsequent calls return the
/// cached path without re-extracting.
///
/// Returns `None` if extraction fails.
fn extracted_typeshed_base_path() -> Option<&'static Path> {
    EXTRACTED_PATH
        .get_or_init(|| match extract_typeshed() {
            Ok(base) => {
                tracing::info!("Extracted vendored typeshed to {}", base.display());
                Some(base)
            }
            Err(err) => {
                tracing::warn!("Failed to extract vendored typeshed: {err}");
                None
            }
        })
        .as_deref()
}

/// Get the path to the extracted typeshed `stdlib` directory on disk.
///
/// Returns `None` if extraction fails.
pub(crate) fn extracted_typeshed_stdlib_path() -> Option<PathBuf> {
    extracted_typeshed_base_path().map(|base| base.join("stdlib"))
}

/// Map a vendored file path (e.g. `stdlib/builtins.pyi` or `stdlib\builtins.pyi`)
/// to its extracted location on disk.
///
/// Returns `None` if the typeshed has not been extracted or the vendored path
/// does not start with `stdlib`.
pub(crate) fn vendored_path_to_disk(vendored_path: &str) -> Option<PathBuf> {
    let base = extracted_typeshed_base_path()?;
    // Handle both forward and backward slashes (Windows vs Unix vendored paths).
    if vendored_path.starts_with("stdlib/")
        || vendored_path.starts_with("stdlib\\")
        || vendored_path == "stdlib"
    {
        // Normalize vendored path separators to the OS separator for disk path joining.
        let normalized = vendored_path.replace('/', std::path::MAIN_SEPARATOR_STR);
        let normalized = normalized.replace('\\', std::path::MAIN_SEPARATOR_STR);
        Some(base.join(normalized))
    } else {
        None
    }
}

/// Extract the vendored typeshed `stdlib/` tree to a temporary directory.
///
/// Returns the base path of the extracted tree (the temp dir root).
/// The tree mirrors the vendored layout, e.g.:
///   `<temp>/stdlib/builtins.pyi`
///   `<temp>/stdlib/os/__init__.pyi`
fn extract_typeshed() -> Result<PathBuf, std::io::Error> {
    let vfs = ty_vendored::file_system();

    // Create a persistent temp directory (not auto-deleted on drop).
    let base_dir = tempfile::Builder::new()
        .prefix("tsp-ty-typeshed-")
        .tempdir()?
        .keep(); // `keep()` prevents cleanup on drop

    // Create the stdlib directory and extract all stubs into it.
    // The vendored typeshed has `stdlib/` as its main directory.
    let stdlib_dir = base_dir.join("stdlib");
    std::fs::create_dir_all(&stdlib_dir)?;
    extract_directory(vfs, VendoredPath::new("stdlib"), &base_dir)?;

    Ok(base_dir)
}

/// Recursively extract a directory from the vendored filesystem to disk.
///
/// `vendored_dir` is the path within the vendored FS (e.g., `"stdlib"`).
/// `disk_base` is the base directory on disk; extracted files are placed at
/// `disk_base / <vendored_path>` to mirror the vendored layout.
fn extract_directory(
    vfs: &VendoredFileSystem,
    vendored_dir: &VendoredPath,
    disk_base: &Path,
) -> Result<(), std::io::Error> {
    let entries = vfs.read_directory(vendored_dir);

    for entry in entries {
        let vendored_path = entry.path();

        // The entry path is the full path within the archive,
        // e.g., "stdlib/builtins.pyi" or "stdlib/os/".
        // Strip trailing slash for directory entries before joining.
        let relative = vendored_path.as_str().trim_end_matches('/');
        let disk_path = disk_base.join(relative);

        match entry.file_type() {
            FileType::Directory => {
                std::fs::create_dir_all(&disk_path)?;
                extract_directory(vfs, vendored_path, disk_base)?;
            }
            FileType::File => {
                // Ensure the parent directory exists
                if let Some(parent) = disk_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                // Read file content from the vendored FS and write to disk
                let content = vfs.read_to_string(vendored_path).map_err(|e| {
                    std::io::Error::other(format!(
                        "Failed to read vendored file {vendored_path}: {e}"
                    ))
                })?;

                std::fs::write(&disk_path, content.as_bytes())?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_typeshed_creates_builtins() {
        let base = extract_typeshed().expect("extraction should succeed");

        // Check that builtins.pyi was extracted
        let builtins = base.join("stdlib").join("builtins.pyi");
        assert!(
            builtins.exists(),
            "builtins.pyi should exist at {builtins:?}"
        );

        // Verify content is non-empty and looks like a stub
        let content = std::fs::read_to_string(&builtins).unwrap();
        assert!(
            content.contains("class int"),
            "builtins.pyi should contain 'class int'"
        );

        // Check that os/__init__.pyi was extracted
        let os_init = base.join("stdlib").join("os").join("__init__.pyi");
        assert!(
            os_init.exists(),
            "os/__init__.pyi should exist at {os_init:?}"
        );

        // Clean up the temp dir
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_extracted_path_returns_stdlib() {
        // Note: this test uses the global cache, so it will extract only once
        // across all tests that call this function.
        let path = extracted_typeshed_stdlib_path();
        assert!(path.is_some(), "should return an extracted path");
        let stdlib = path.unwrap();
        assert!(
            stdlib.ends_with("stdlib"),
            "path should end with 'stdlib': {stdlib:?}"
        );
        assert!(
            stdlib.join("builtins.pyi").exists(),
            "builtins.pyi should exist under the stdlib path"
        );
    }

    #[test]
    fn test_vendored_path_to_disk_maps_stdlib() {
        let disk_path = vendored_path_to_disk("stdlib/builtins.pyi");
        assert!(disk_path.is_some(), "should map stdlib vendored path");
        let path = disk_path.unwrap();
        assert!(path.exists(), "mapped path should exist: {path:?}");
        assert!(
            path.ends_with("stdlib/builtins.pyi") || path.ends_with("stdlib\\builtins.pyi"),
            "path should end with stdlib/builtins.pyi: {path:?}"
        );
    }

    #[test]
    fn test_vendored_path_to_disk_rejects_non_stdlib() {
        let disk_path = vendored_path_to_disk("some_other/path.pyi");
        assert!(
            disk_path.is_none(),
            "should not map non-stdlib vendored paths"
        );
    }
}
