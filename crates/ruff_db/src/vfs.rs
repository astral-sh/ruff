mod path;

use std::sync::Arc;

use countme::Count;
use dashmap::mapref::entry::Entry;
use filetime::FileTime;

use crate::{Db, FxDashMap};

pub use path::VfsPath;

#[salsa::input]
pub struct VfsFile {
    /// The path of the file.
    #[id]
    #[return_ref]
    pub path: VfsPath,

    /// The unix permissions of the file. Only supported on unix systems. Always 0 on Windows
    /// or when the file has been deleted.
    pub permissions: Option<u32>,

    /// The file revision. A file has changed if the revisions don't compare equal.
    pub revision: FileRevision,

    /// The status of the file.
    ///
    /// Salsa doesn't support deleting inputs. The only way to signal to the depending queries that
    /// the file has been deleted is to change the status to `Deleted`.
    pub status: FileStatus,

    /// Counter that counts the number of created file instances and active file instances.
    /// Only enabled in debug builds.
    #[allow(unused)]
    count: Count<VfsFile>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FileStatus {
    /// The file exists.
    Exists,

    /// The file was deleted, didn't exist to begin with or the path isn't a file.
    Deleted,
}

/// A number representing the revision of a file.
///
/// Two revisions that don't compare equal signify that the file has been modified.
/// Revisions aren't guaranteed to be monotonically increasing or in any specific order.
///
/// Possible revisions are:
/// * The last modification time of the file.
/// * The hash of the file's content.
/// * The revision as it comes from an external system, for example the LSP.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FileRevision(u128);

impl FileRevision {
    pub fn new(value: u128) -> Self {
        Self(value)
    }

    pub const fn zero() -> Self {
        Self(0)
    }

    #[must_use]
    pub fn as_u128(self) -> u128 {
        self.0
    }
}

impl From<u128> for FileRevision {
    fn from(value: u128) -> Self {
        FileRevision(value)
    }
}

impl From<u64> for FileRevision {
    fn from(value: u64) -> Self {
        FileRevision(u128::from(value))
    }
}

impl From<FileTime> for FileRevision {
    fn from(value: FileTime) -> Self {
        let seconds = value.seconds() as u128;
        let seconds = seconds << 64;
        let nanos = value.nanoseconds() as u128;

        FileRevision(seconds | nanos)
    }
}

/// Virtual file system that tracks the metadata of files.
#[derive(Default)]
pub struct Vfs {
    /// Lookup table that maps the path to a salsa interned [`VfsFile`] instance.
    ///
    /// The map also stores entries for files that don't exist on the file system. This is necessary
    /// so that queries that depend on the existence of a file are re-executed when the file is created.
    files_by_path: Arc<FxDashMap<VfsPath, VfsFile>>,
}

impl Vfs {
    /// Looks up a file by its path.
    ///
    /// For a non-existing file, creates a new salsa `File` ingredient and stores it for future lookups.
    ///
    /// The operation always succeeds even if the path doesn't exist on disk, isn't accessible or if the path points to a directory.
    /// In these cases, a file with status [`FileStatus::Deleted`] is returned.
    pub fn fs(&self, db: &dyn Db, path: &camino::Utf8Path) -> VfsFile {
        match self.files_by_path.entry(VfsPath::Fs(path.to_path_buf())) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(vacant) => {
                let metadata = std::fs::metadata(path);
                let file_path = VfsPath::fs(path.to_path_buf());

                let file = match metadata {
                    Ok(metadata) if metadata.file_type().is_file() => {
                        let last_modified = FileTime::from_last_modification_time(&metadata);
                        let permission = if cfg!(unix) {
                            use std::os::unix::fs::PermissionsExt;

                            Some(metadata.permissions().mode())
                        } else {
                            None
                        };

                        VfsFile::new(
                            db,
                            file_path,
                            permission,
                            last_modified.into(),
                            FileStatus::Exists,
                            Count::default(),
                        )
                    }
                    _ => VfsFile::new(
                        db,
                        file_path,
                        None,
                        FileRevision::zero(),
                        FileStatus::Deleted,
                        Count::default(),
                    ),
                };

                vacant.insert(file);
                file
            }
        }
    }

    pub fn vendored(&self, _db: &dyn Db, _path: &camino::Utf8Path) -> Option<VfsFile> {
        // TODO: Lookup the path in the vendored file system
        None
    }

    /// Creates a salsa like snapshot of the files. The instances share
    /// the same path to file mapping.
    pub fn snapshot(&self) -> Self {
        Self {
            files_by_path: self.files_by_path.clone(),
        }
    }
}

impl std::fmt::Debug for Vfs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();

        for entry in self.files_by_path.iter() {
            map.entry(entry.key(), entry.value());
        }
        map.finish()
    }
}
