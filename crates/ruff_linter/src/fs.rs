use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

use path_absolutize::Absolutize;

use crate::registry::RuleSet;
use crate::settings::types::CompiledPerFileIgnoreList;

/// Atomically write `contents` to `path`.
///
/// Writes to a temporary file in the same directory as `path`, then renames
/// the temporary file over `path`. `rename(2)` is atomic on POSIX, so an
/// interrupted write leaves the original file intact.
pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    let mut temp = tempfile_in(parent)?;
    temp.write_all(contents)?;
    temp.persist(path).map_err(|err| err.error).map(drop)
}

/// Return a [`NamedTempFile`] in the specified directory.
///
/// Sets permissions to `0o666` on Unix (matching the non-temporary file
/// default; [`NamedTempFile`] otherwise defaults to `0o600`).
#[cfg(unix)]
pub(crate) fn tempfile_in(path: &Path) -> io::Result<NamedTempFile> {
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o666))
        .tempfile_in(path)
}

/// Return a [`NamedTempFile`] in the specified directory.
#[cfg(not(unix))]
pub(crate) fn tempfile_in(path: &Path) -> io::Result<NamedTempFile> {
    tempfile::Builder::new().tempfile_in(path)
}

/// Return the current working directory.
///
/// On WASM this just returns `.`. Otherwise, defer to [`path_absolutize::path_dedot::CWD`].
pub fn get_cwd() -> &'static Path {
    #[cfg(target_arch = "wasm32")]
    {
        Path::new(".")
    }
    #[cfg(not(target_arch = "wasm32"))]
    path_absolutize::path_dedot::CWD.as_path()
}

/// Create a set with codes matching the pattern/code pairs.
pub(crate) fn ignores_from_path(path: &Path, ignore_list: &CompiledPerFileIgnoreList) -> RuleSet {
    if ignore_list.is_empty() {
        return RuleSet::empty();
    }
    ignore_list
        .iter_matches(path, "Adding per-file ignores")
        .flatten()
        .collect()
}

/// Convert any path to an absolute path (based on the current working
/// directory).
pub fn normalize_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();
    if let Ok(path) = path.absolutize() {
        return path.to_path_buf();
    }
    path.to_path_buf()
}

/// Convert any path to an absolute path (based on the specified project root).
pub fn normalize_path_to<P: AsRef<Path>, R: AsRef<Path>>(path: P, project_root: R) -> PathBuf {
    let path = path.as_ref();
    if let Ok(path) = path.absolutize_from(project_root.as_ref()) {
        return path.to_path_buf();
    }
    path.to_path_buf()
}

/// Convert an absolute path to be relative to the current working directory.
pub fn relativize_path<P: AsRef<Path>>(path: P) -> String {
    let path = path.as_ref();

    let cwd = get_cwd();
    if let Ok(path) = path.strip_prefix(cwd) {
        return format!("{}", path.display());
    }
    format!("{}", path.display())
}
