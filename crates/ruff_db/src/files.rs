use std::sync::Arc;

use countme::Count;
use dashmap::mapref::entry::Entry;

pub use path::FilePath;

use crate::file_revision::FileRevision;
use crate::files::private::FileStatus;
use crate::system::SystemPath;
use crate::vendored::VendoredPath;
use crate::{Db, FxDashMap};

mod path;

/// Interns a file system path and returns a salsa `File` ingredient.
///
/// Returns `None` if the path doesn't exist, isn't accessible, or if the path points to a directory.
#[inline]
pub fn system_path_to_file(db: &dyn Db, path: impl AsRef<SystemPath>) -> Option<File> {
    let file = db.files().system(db, path.as_ref());

    // It's important that `vfs.file_system` creates a `VfsFile` even for files that don't exist or don't
    // exist anymore so that Salsa can track that the caller of this function depends on the existence of
    // that file. This function filters out files that don't exist, but Salsa will know that it must
    // re-run the calling query whenever the `file`'s status changes (because of the `.status` call here).
    match file.status(db) {
        FileStatus::Exists => Some(file),
        FileStatus::Deleted => None,
    }
}

/// Interns a vendored file path. Returns `Some` if the vendored file for `path` exists and `None` otherwise.
#[inline]
pub fn vendored_path_to_file(db: &dyn Db, path: impl AsRef<VendoredPath>) -> Option<File> {
    db.files().vendored(db, path.as_ref())
}

/// Lookup table that maps [file paths](`FilePath`) to salsa interned [`File`] instances.
#[derive(Default)]
pub struct Files {
    inner: Arc<FilesInner>,
}

#[derive(Default)]
struct FilesInner {
    /// Lookup table that maps [`FilePath`]s to salsa interned [`File`] instances.
    ///
    /// The map also stores entries for files that don't exist on the file system. This is necessary
    /// so that queries that depend on the existence of a file are re-executed when the file is created.
    files_by_path: FxDashMap<FilePath, File>,
}

impl Files {
    /// Looks up a file by its `path`.
    ///
    /// For a non-existing file, creates a new salsa [`File`] ingredient and stores it for future lookups.
    ///
    /// The operation always succeeds even if the path doesn't exist on disk, isn't accessible or if the path points to a directory.
    /// In these cases, a file with status [`FileStatus::Deleted`] is returned.
    #[tracing::instrument(level = "debug", skip(self, db))]
    fn system(&self, db: &dyn Db, path: &SystemPath) -> File {
        let absolute = SystemPath::absolute(path, db.system().current_directory());
        let absolute = FilePath::System(absolute);

        *self
            .inner
            .files_by_path
            .entry(absolute.clone())
            .or_insert_with(|| {
                let metadata = db.system().path_metadata(path);

                match metadata {
                    Ok(metadata) if metadata.file_type().is_file() => File::new(
                        db,
                        absolute,
                        metadata.permissions(),
                        metadata.revision(),
                        FileStatus::Exists,
                        Count::default(),
                    ),
                    _ => File::new(
                        db,
                        absolute,
                        None,
                        FileRevision::zero(),
                        FileStatus::Deleted,
                        Count::default(),
                    ),
                }
            })
    }

    /// Tries to look up the file for the given system path, returns `None` if no such file exists yet
    fn try_system(&self, db: &dyn Db, path: &SystemPath) -> Option<File> {
        let absolute = SystemPath::absolute(path, db.system().current_directory());
        self.inner
            .files_by_path
            .get(&FilePath::System(absolute))
            .map(|entry| *entry.value())
    }

    /// Looks up a vendored file by its path. Returns `Some` if a vendored file for the given path
    /// exists and `None` otherwise.
    #[tracing::instrument(level = "debug", skip(self, db))]
    fn vendored(&self, db: &dyn Db, path: &VendoredPath) -> Option<File> {
        let file = match self
            .inner
            .files_by_path
            .entry(FilePath::Vendored(path.to_path_buf()))
        {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let metadata = db.vendored().metadata(path).ok()?;

                let file = File::new(
                    db,
                    FilePath::Vendored(path.to_path_buf()),
                    Some(0o444),
                    metadata.revision(),
                    FileStatus::Exists,
                    Count::default(),
                );

                entry.insert(file);

                file
            }
        };

        Some(file)
    }

    /// Creates a salsa like snapshot. The instances share
    /// the same path-to-file mapping.
    pub fn snapshot(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl std::fmt::Debug for Files {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();

        for entry in self.inner.files_by_path.iter() {
            map.entry(entry.key(), entry.value());
        }
        map.finish()
    }
}

/// A file that's either stored on the host system's file system or in the vendored file system.
#[salsa::input]
pub struct File {
    /// The path of the file.
    #[id]
    #[return_ref]
    pub path: FilePath,

    /// The unix permissions of the file. Only supported on unix systems. Always `None` on Windows
    /// or when the file has been deleted.
    pub permissions: Option<u32>,

    /// The file revision. A file has changed if the revisions don't compare equal.
    pub revision: FileRevision,

    /// The status of the file.
    ///
    /// Salsa doesn't support deleting inputs. The only way to signal dependent queries that
    /// the file has been deleted is to change the status to `Deleted`.
    status: FileStatus,

    /// Counter that counts the number of created file instances and active file instances.
    /// Only enabled in debug builds.
    #[allow(unused)]
    count: Count<File>,
}

impl File {
    /// Reads the content of the file into a [`String`].
    ///
    /// Reading the same file multiple times isn't guaranteed to return the same content. It's possible
    /// that the file has been modified in between the reads. It's even possible that a file that
    /// is considered to exist has been deleted in the meantime. If this happens, then the method returns
    /// an empty string, which is the closest to the content that the file contains now. Returning
    /// an empty string shouldn't be a problem because the query will be re-executed as soon as the
    /// changes are applied to the database.
    pub(crate) fn read_to_string(&self, db: &dyn Db) -> String {
        let path = self.path(db);

        let result = match path {
            FilePath::System(system) => {
                // Add a dependency on the revision to ensure the operation gets re-executed when the file changes.
                let _ = self.revision(db);

                db.system().read_to_string(system)
            }
            FilePath::Vendored(vendored) => db.vendored().read_to_string(vendored),
        };

        result.unwrap_or_default()
    }

    /// Refreshes the file metadata by querying the file system if needed.
    /// TODO: The API should instead take all observed changes from the file system directly
    ///   and then apply the VfsFile status accordingly. But for now, this is sufficient.
    pub fn touch_path(db: &mut dyn Db, path: &FilePath) {
        Self::touch_impl(db, path, None);
    }

    pub fn touch(self, db: &mut dyn Db) {
        let path = self.path(db).clone();
        Self::touch_impl(db, &path, Some(self));
    }

    /// Private method providing the implementation for [`Self::touch_path`] and [`Self::touch`].
    fn touch_impl(db: &mut dyn Db, path: &FilePath, file: Option<File>) {
        match path {
            FilePath::System(path) => {
                let metadata = db.system().path_metadata(path);

                let (status, revision) = match metadata {
                    Ok(metadata) if metadata.file_type().is_file() => {
                        (FileStatus::Exists, metadata.revision())
                    }
                    _ => (FileStatus::Deleted, FileRevision::zero()),
                };

                let Some(file) = file.or_else(|| db.files().try_system(db, path)) else {
                    return;
                };

                file.set_status(db).to(status);
                file.set_revision(db).to(revision);
            }
            FilePath::Vendored(_) => {
                // Readonly, can never be out of date.
            }
        }
    }
}

// The types in here need to be public because they're salsa ingredients but we
// don't want them to be publicly accessible. That's why we put them into a private module.
mod private {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum FileStatus {
        /// The file exists.
        Exists,

        /// The file was deleted, didn't exist to begin with or the path isn't a file.
        Deleted,
    }
}

#[cfg(test)]
mod tests {
    use crate::file_revision::FileRevision;
    use crate::files::{system_path_to_file, vendored_path_to_file};
    use crate::system::DbWithTestSystem;
    use crate::tests::TestDb;
    use crate::vendored::tests::VendoredFileSystemBuilder;

    #[test]
    fn system_existing_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();

        db.write_file("test.py", "print('Hello world')")?;

        let test = system_path_to_file(&db, "test.py").expect("File to exist.");

        assert_eq!(test.permissions(&db), Some(0o755));
        assert_ne!(test.revision(&db), FileRevision::zero());
        assert_eq!(&test.read_to_string(&db), "print('Hello world')");

        Ok(())
    }

    #[test]
    fn system_non_existing_file() {
        let db = TestDb::new();

        let test = system_path_to_file(&db, "test.py");

        assert_eq!(test, None);
    }

    #[test]
    fn system_normalize_paths() {
        let db = TestDb::new();

        assert_eq!(
            system_path_to_file(&db, "test.py"),
            system_path_to_file(&db, "/test.py")
        );

        assert_eq!(
            system_path_to_file(&db, "/root/.././test.py"),
            system_path_to_file(&db, "/root/test.py")
        );
    }

    #[test]
    fn stubbed_vendored_file() {
        let mut db = TestDb::new();

        let mut vendored_builder = VendoredFileSystemBuilder::new();
        vendored_builder
            .add_file("test.pyi", "def foo() -> str")
            .unwrap();
        let vendored = vendored_builder.finish().unwrap();
        db.with_vendored(vendored);

        let test = vendored_path_to_file(&db, "test.pyi").expect("Vendored file to exist.");

        assert_eq!(test.permissions(&db), Some(0o444));
        assert_ne!(test.revision(&db), FileRevision::zero());
        assert_eq!(&test.read_to_string(&db), "def foo() -> str");
    }

    #[test]
    fn stubbed_vendored_file_non_existing() {
        let db = TestDb::new();

        assert_eq!(vendored_path_to_file(&db, "test.py"), None);
    }
}
