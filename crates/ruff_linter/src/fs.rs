use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use path_absolutize::Absolutize;

use crate::registry::RuleSet;
use crate::settings::types::CompiledPerFileIgnoreList;

/// Atomically write `contents` to `path`.
///
/// If `path` is a symbolic link, it is resolved first so the write applies to
/// the target file.
/// If `path` is a hard link, it is written to directly to avoid breaking the link.
/// Otherwise, it writes to a temporary file in the same directory as `path`,
/// preserving the existing file permissions if it exists (defaulting to 0o666 subject
/// to umask on Unix if the file is new), then renames the temporary file over `path`.
pub fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let resolved_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let metadata = std::fs::metadata(&resolved_path).ok();

    if let Some(ref meta) = metadata {
        if is_hard_link(meta) {
            let mut file = std::fs::File::create(&resolved_path)?;
            file.write_all(contents)?;
            return Ok(());
        }
    }

    let parent = resolved_path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;

    #[cfg(unix)]
    let builder = {
        let mut builder = tempfile::Builder::new();
        if let Some(ref meta) = metadata {
            builder.permissions(meta.permissions());
        } else {
            builder.permissions(std::fs::Permissions::from_mode(0o666));
        }
        builder
    };
    #[cfg(not(unix))]
    let builder = tempfile::Builder::new();

    let mut temp = builder.tempfile_in(parent)?;

    #[cfg(windows)]
    {
        if let Some(ref meta) = metadata {
            let _ = temp.as_file().set_permissions(meta.permissions());
        }
    }

    temp.write_all(contents)?;
    temp.persist(&resolved_path)
        .map_err(|err| err.error)
        .map(drop)
}

#[cfg(unix)]
fn is_hard_link(metadata: &std::fs::Metadata) -> bool {
    metadata.nlink() > 1
}

#[cfg(not(unix))]
fn is_hard_link(_metadata: &std::fs::Metadata) -> bool {
    false
}

/// Return the current working directory.
///
/// On WASM this just returns `.`. Otherwise, defer to [`path_absolutize::path_dedot::CWD`].
pub fn get_cwd() -> &'static Path {
    cfg_select! {
        target_arch = "wasm32" => Path::new("."),
        _ => path_absolutize::path_dedot::CWD.as_path(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_new_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("new_file.py");
        atomic_write(&path, b"print('hello')").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "print('hello')");
    }

    #[test]
    #[cfg(unix)]
    fn preserves_permissions() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("perm_test.py");
        std::fs::write(&path, b"original").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();

        atomic_write(&path, b"updated").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "updated");
        let metadata = std::fs::metadata(&path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
    }

    #[test]
    #[cfg(unix)]
    fn respects_symlinks() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("target.py");
        let symlink = dir.path().join("link.py");

        std::fs::write(&target, b"target content").unwrap();
        std::os::unix::fs::symlink(&target, &symlink).unwrap();

        atomic_write(&symlink, b"new content").unwrap();

        // Target should be updated
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new content");
        // Link should still be a symlink pointing to target
        let metadata = std::fs::symlink_metadata(&symlink).unwrap();
        assert!(metadata.is_symlink());
        assert_eq!(std::fs::read_link(&symlink).unwrap(), target);
    }

    #[test]
    #[cfg(unix)]
    fn respects_hardlinks() {
        let dir = tempdir().unwrap();
        let original = dir.path().join("original.py");
        let link = dir.path().join("link.py");

        std::fs::write(&original, b"original content").unwrap();
        std::fs::hard_link(&original, &link).unwrap();

        atomic_write(&link, b"new content").unwrap();

        // Both original and link should be updated to new content
        assert_eq!(std::fs::read_to_string(&original).unwrap(), "new content");
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "new content");

        // The hard link connection should NOT be severed
        assert_eq!(std::fs::metadata(&original).unwrap().nlink(), 2);
        assert_eq!(std::fs::metadata(&link).unwrap().nlink(), 2);
    }
}
