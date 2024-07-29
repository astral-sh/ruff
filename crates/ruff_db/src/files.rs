use std::fmt::Formatter;
use std::sync::Arc;

use countme::Count;
use dashmap::mapref::entry::Entry;
use salsa::{Durability, Setter};

pub use file_root::{FileRoot, FileRootKind};
pub use path::FilePath;
use ruff_notebook::{Notebook, NotebookError};

use crate::file_revision::FileRevision;
use crate::files::file_root::FileRoots;
use crate::files::private::FileStatus;
use crate::system::{Metadata, SystemPath, SystemPathBuf, SystemVirtualPath, SystemVirtualPathBuf};
use crate::vendored::{VendoredPath, VendoredPathBuf};
use crate::{Db, FxDashMap};

mod file_root;
mod path;

/// Interns a file system path and returns a salsa `File` ingredient.
///
/// Returns `Err` if the path doesn't exist, isn't accessible, or if the path points to a directory.
#[inline]
pub fn system_path_to_file(
    db: &dyn Db,
    path: impl AsRef<SystemPath>,
) -> Result<File, SystemPathError> {
    let file = db.files().system(db, path.as_ref());

    // It's important that `vfs.file_system` creates a `VfsFile` even for files that don't exist or don't
    // exist anymore so that Salsa can track that the caller of this function depends on the existence of
    // that file. This function filters out files that don't exist, but Salsa will know that it must
    // re-run the calling query whenever the `file`'s status changes (because of the `.status` call here).
    match file.status(db) {
        FileStatus::Exists => Ok(file),
        FileStatus::IsADirectory => Err(SystemPathError::IsADirectory),
        FileStatus::NotFound => Err(SystemPathError::NotFound),
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
    /// Lookup table that maps [`SystemPathBuf`]s to salsa interned [`File`] instances.
    ///
    /// The map also stores entries for files that don't exist on the file system. This is necessary
    /// so that queries that depend on the existence of a file are re-executed when the file is created.
    system_by_path: FxDashMap<SystemPathBuf, File>,

    /// Lookup table that maps [`SystemVirtualPathBuf`]s to salsa interned [`File`] instances.
    system_virtual_by_path: FxDashMap<SystemVirtualPathBuf, File>,

    /// Lookup table that maps vendored files to the salsa [`File`] ingredients.
    vendored_by_path: FxDashMap<VendoredPathBuf, File>,

    /// Lookup table that maps file paths to their [`FileRoot`].
    roots: std::sync::RwLock<FileRoots>,
}

impl Files {
    /// Looks up a file by its `path`.
    ///
    /// For a non-existing file, creates a new salsa [`File`] ingredient and stores it for future lookups.
    ///
    /// The operation always succeeds even if the path doesn't exist on disk, isn't accessible or if the path points to a directory.
    /// In these cases, a file with status [`FileStatus::NotFound`] is returned.
    #[tracing::instrument(level = "trace", skip(self, db))]
    fn system(&self, db: &dyn Db, path: &SystemPath) -> File {
        let absolute = SystemPath::absolute(path, db.system().current_directory());

        *self
            .inner
            .system_by_path
            .entry(absolute.clone())
            .or_insert_with(|| {
                let metadata = db.system().path_metadata(path);
                let durability = self
                    .root(db, path)
                    .map_or(Durability::default(), |root| root.durability(db));

                let (permissions, revision, status) = match metadata {
                    Ok(metadata) if metadata.file_type().is_file() => (
                        metadata.permissions(),
                        metadata.revision(),
                        FileStatus::Exists,
                    ),
                    Ok(metadata) if metadata.file_type().is_directory() => {
                        (None, FileRevision::zero(), FileStatus::IsADirectory)
                    }
                    _ => (None, FileRevision::zero(), FileStatus::NotFound),
                };

                File::builder(
                    FilePath::System(absolute),
                    permissions,
                    revision,
                    status,
                    Count::default(),
                )
                .durability(durability)
                .new(db)
            })
    }

    /// Tries to look up the file for the given system path, returns `None` if no such file exists yet
    pub fn try_system(&self, db: &dyn Db, path: &SystemPath) -> Option<File> {
        let absolute = SystemPath::absolute(path, db.system().current_directory());
        self.inner
            .system_by_path
            .get(&absolute)
            .map(|entry| *entry.value())
    }

    /// Looks up a vendored file by its path. Returns `Some` if a vendored file for the given path
    /// exists and `None` otherwise.
    #[tracing::instrument(level = "trace", skip(self, db))]
    fn vendored(&self, db: &dyn Db, path: &VendoredPath) -> Option<File> {
        let file = match self.inner.vendored_by_path.entry(path.to_path_buf()) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let metadata = db.vendored().metadata(path).ok()?;

                let file = File::builder(
                    FilePath::Vendored(path.to_path_buf()),
                    Some(0o444),
                    metadata.revision(),
                    FileStatus::Exists,
                    Count::default(),
                )
                .durability(Durability::HIGH)
                .new(db);

                entry.insert(file);

                file
            }
        };

        Some(file)
    }

    /// Looks up a virtual file by its `path`.
    ///
    /// For a non-existing file, creates a new salsa [`File`] ingredient and stores it for future lookups.
    ///
    /// The operations fails if the system failed to provide a metadata for the path.
    #[tracing::instrument(level = "trace", skip(self, db), ret)]
    pub fn add_virtual_file(&self, db: &dyn Db, path: &SystemVirtualPath) -> Option<File> {
        let file = match self.inner.system_virtual_by_path.entry(path.to_path_buf()) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let metadata = db.system().virtual_path_metadata(path).ok()?;

                let file = File::new(
                    db,
                    FilePath::SystemVirtual(path.to_path_buf()),
                    metadata.permissions(),
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

    /// Looks up the closest  root for `path`. Returns `None` if `path` isn't enclosed by any source root.
    ///
    /// Roots can be nested, in which case the closest root is returned.
    pub fn root(&self, db: &dyn Db, path: &SystemPath) -> Option<FileRoot> {
        let roots = self.inner.roots.read().unwrap();

        let absolute = SystemPath::absolute(path, db.system().current_directory());
        roots.at(&absolute)
    }

    /// Adds a new root for `path` and returns the root.
    ///
    /// The root isn't added nor is the file root's kind updated if a root for `path` already exists.
    pub fn try_add_root(&self, db: &dyn Db, path: &SystemPath, kind: FileRootKind) -> FileRoot {
        let mut roots = self.inner.roots.write().unwrap();

        let absolute = SystemPath::absolute(path, db.system().current_directory());
        roots.try_add(db, absolute, kind)
    }

    /// Updates the revision of the root for `path`.
    pub fn touch_root(db: &mut dyn Db, path: &SystemPath) {
        if let Some(root) = db.files().root(db, path) {
            root.set_revision(db).to(FileRevision::now());
        }
    }

    /// Refreshes the state of all known files under `path` recursively.
    ///
    /// The most common use case is to update the [`Files`] state after removing or moving a directory.
    ///
    /// # Performance
    /// Refreshing the state of every file under `path` is expensive. It requires iterating over all known files
    /// and making system calls to get the latest status of each file in `path`.
    /// That's why [`File::sync_path`] and [`File::sync_path`] is preferred if it is known that the path is a file.
    #[tracing::instrument(level = "debug", skip(db))]
    pub fn sync_recursively(db: &mut dyn Db, path: &SystemPath) {
        let path = SystemPath::absolute(path, db.system().current_directory());

        let inner = Arc::clone(&db.files().inner);
        for entry in inner.system_by_path.iter_mut() {
            if entry.key().starts_with(&path) {
                File::sync_system_path(db, entry.key(), Some(*entry.value()));
            }
        }

        let roots = inner.roots.read().unwrap();

        for root in roots.all() {
            if root.path(db).starts_with(&path) {
                root.set_revision(db)
                    .with_durability(Durability::HIGH)
                    .to(FileRevision::now());
            }
        }
    }

    /// Refreshes the state of all known files.
    ///
    /// This is a last-resort method that should only be used when more granular updates aren't possible
    /// (for example, because the file watcher failed to observe some changes). Use responsibly!
    ///
    /// # Performance
    /// Refreshing the state of every file is expensive. It requires iterating over all known files and
    /// issuing a system call to get the latest status of each file.
    #[tracing::instrument(level = "debug", skip(db))]
    pub fn sync_all(db: &mut dyn Db) {
        let inner = Arc::clone(&db.files().inner);
        for entry in inner.system_by_path.iter_mut() {
            File::sync_system_path(db, entry.key(), Some(*entry.value()));
        }

        let roots = inner.roots.read().unwrap();

        for root in roots.all() {
            root.set_revision(db)
                .with_durability(Durability::HIGH)
                .to(FileRevision::now());
        }
    }
}

impl std::fmt::Debug for Files {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();

        for entry in self.inner.system_by_path.iter() {
            map.entry(entry.key(), entry.value());
        }
        map.finish()
    }
}

/// A file that's either stored on the host system's file system or in the vendored file system.
#[salsa::input]
pub struct File {
    /// The path of the file.
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
    /// that the file has been modified in between the reads.
    pub fn read_to_string(&self, db: &dyn Db) -> crate::system::Result<String> {
        let path = self.path(db);

        match path {
            FilePath::System(system) => {
                // Add a dependency on the revision to ensure the operation gets re-executed when the file changes.
                let _ = self.revision(db);

                db.system().read_to_string(system)
            }
            FilePath::Vendored(vendored) => db.vendored().read_to_string(vendored),
            FilePath::SystemVirtual(system_virtual) => {
                db.system().read_virtual_path_to_string(system_virtual)
            }
        }
    }

    /// Reads the content of the file into a [`Notebook`].
    ///
    /// Reading the same file multiple times isn't guaranteed to return the same content. It's possible
    /// that the file has been modified in between the reads.
    pub fn read_to_notebook(&self, db: &dyn Db) -> Result<Notebook, NotebookError> {
        let path = self.path(db);

        match path {
            FilePath::System(system) => {
                // Add a dependency on the revision to ensure the operation gets re-executed when the file changes.
                let _ = self.revision(db);

                db.system().read_to_notebook(system)
            }
            FilePath::Vendored(_) => Err(NotebookError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Reading a notebook from the vendored file system is not supported.",
            ))),
            FilePath::SystemVirtual(system_virtual) => {
                db.system().read_virtual_path_to_notebook(system_virtual)
            }
        }
    }

    /// Refreshes the file metadata by querying the file system if needed.
    #[tracing::instrument(level = "debug", skip(db))]
    pub fn sync_path(db: &mut dyn Db, path: &SystemPath) {
        let absolute = SystemPath::absolute(path, db.system().current_directory());
        Files::touch_root(db, &absolute);
        Self::sync_system_path(db, &absolute, None);
    }

    /// Syncs the [`File`]'s state with the state of the file on the system.
    #[tracing::instrument(level = "debug", skip(db))]
    pub fn sync(self, db: &mut dyn Db) {
        let path = self.path(db).clone();

        match path {
            FilePath::System(system) => {
                Files::touch_root(db, &system);
                Self::sync_system_path(db, &system, Some(self));
            }
            FilePath::Vendored(_) => {
                // Readonly, can never be out of date.
            }
            FilePath::SystemVirtual(system_virtual) => {
                Self::sync_system_virtual_path(db, &system_virtual, self);
            }
        }
    }

    fn sync_system_path(db: &mut dyn Db, path: &SystemPath, file: Option<File>) {
        let Some(file) = file.or_else(|| db.files().try_system(db, path)) else {
            return;
        };
        let metadata = db.system().path_metadata(path);
        let durability = db.files().root(db, path).map(|root| root.durability(db));
        Self::sync_impl(db, metadata, file, durability);
    }

    fn sync_system_virtual_path(db: &mut dyn Db, path: &SystemVirtualPath, file: File) {
        let metadata = db.system().virtual_path_metadata(path);
        Self::sync_impl(db, metadata, file, None);
    }

    /// Private method providing the implementation for [`Self::sync_system_path`] and
    /// [`Self::sync_system_virtual_path`].
    fn sync_impl(
        db: &mut dyn Db,
        metadata: crate::system::Result<Metadata>,
        file: File,
        durability: Option<Durability>,
    ) {
        let (status, revision, permission) = match metadata {
            Ok(metadata) if metadata.file_type().is_file() => (
                FileStatus::Exists,
                metadata.revision(),
                metadata.permissions(),
            ),
            Ok(metadata) if metadata.file_type().is_directory() => {
                (FileStatus::IsADirectory, FileRevision::zero(), None)
            }
            _ => (FileStatus::NotFound, FileRevision::zero(), None),
        };

        let durability = durability.unwrap_or_default();

        if file.status(db) != status {
            file.set_status(db).with_durability(durability).to(status);
        }

        if file.revision(db) != revision {
            file.set_revision(db)
                .with_durability(durability)
                .to(revision);
        }

        if file.permissions(db) != permission {
            file.set_permissions(db)
                .with_durability(durability)
                .to(permission);
        }
    }

    /// Returns `true` if the file exists.
    pub fn exists(self, db: &dyn Db) -> bool {
        self.status(db) == FileStatus::Exists
    }
}

// The types in here need to be public because they're salsa ingredients but we
// don't want them to be publicly accessible. That's why we put them into a private module.
mod private {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub enum FileStatus {
        /// The file exists.
        Exists,

        /// The path isn't a file and instead points to a directory.
        IsADirectory,

        /// The path doesn't exist, isn't accessible, or no longer exists.
        NotFound,
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SystemPathError {
    IsADirectory,
    NotFound,
}

impl std::fmt::Display for SystemPathError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SystemPathError::IsADirectory => f.write_str("Is a directory"),
            SystemPathError::NotFound => f.write_str("Not found"),
        }
    }
}

impl std::error::Error for SystemPathError {}

#[cfg(test)]
mod tests {
    use crate::file_revision::FileRevision;
    use crate::files::{system_path_to_file, vendored_path_to_file, SystemPathError};
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
        assert_eq!(&test.read_to_string(&db)?, "print('Hello world')");

        Ok(())
    }

    #[test]
    fn system_non_existing_file() {
        let db = TestDb::new();

        let test = system_path_to_file(&db, "test.py");

        assert_eq!(test, Err(SystemPathError::NotFound));
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
    fn stubbed_vendored_file() -> crate::system::Result<()> {
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
        assert_eq!(&test.read_to_string(&db)?, "def foo() -> str");

        Ok(())
    }

    #[test]
    fn stubbed_vendored_file_non_existing() {
        let db = TestDb::new();

        assert_eq!(vendored_path_to_file(&db, "test.py"), None);
    }
}
