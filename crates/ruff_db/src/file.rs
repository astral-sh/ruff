use std::sync::Arc;

use countme::Count;

use crate::vfs::FileRevision;
use crate::{vfs::VfsPath, Db, FxDashMap};

#[salsa::input]
pub struct File {
    /// The path of the file.
    #[id]
    #[return_ref]
    pub path: VfsPath,

    /// The unix permissions of the file. Only supported on unix systems. Always 0 on Windows
    /// or when the file has been deleted.
    pub permissions: u32,

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
    count: Count<File>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FileStatus {
    /// The file exists.
    Exists,

    /// The file was deleted, didn't exist to begin with or the path isn't a file.
    Deleted,
}

#[derive(Default)]
pub struct Files {
    /// Lookup table that maps the path to a salsa interned `File` instance.
    by_path: Arc<FxDashMap<VfsPath, File>>,
}

impl std::fmt::Debug for Files {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();

        for entry in self.by_path.iter() {
            map.entry(entry.key(), entry.value());
        }
        map.finish()
    }
}

impl Files {
    /// Looks up a file by its path.
    ///
    /// For a non-existing file, creates a new salsa `File` ingredient and stores it for future lookups.
    ///
    /// The operation always succeeds even if the path doesn't exist on disk, isn't accessible or if the path points to a directory.
    /// In these cases, a file with status [`FileStatus::Deleted`] is returned.
    pub fn lookup(&self, db: &dyn Db, path: VfsPath) -> File {
        *self.by_path.entry(path.clone()).or_insert_with(|| {
            let metadata = db.vfs().metadata(&path);

            if let Ok(metadata) = metadata {
                // TODO: Set a longer durability for std files.

                File::new(
                    db,
                    path,
                    metadata.permission(),
                    metadata.revision(),
                    FileStatus::Exists,
                    Count::default(),
                )
            } else {
                File::new(
                    db,
                    path,
                    0,
                    FileRevision::zero(),
                    FileStatus::Deleted,
                    Count::default(),
                )
            }
        })
    }

    /// Creates a salsa like snapshot of the files. The instances share
    /// the same path to file mapping.
    pub fn snapshot(&self) -> Self {
        Self {
            by_path: self.by_path.clone(),
        }
    }
}
