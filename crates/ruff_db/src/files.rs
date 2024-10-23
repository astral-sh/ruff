use std::fmt::Formatter;
use std::sync::Arc;

use countme::Count;
use dashmap::mapref::entry::Entry;
use salsa::{Durability, Setter};

pub use file_root::{FileRoot, FileRootKind};
pub use path::FilePath;
use ruff_notebook::{Notebook, NotebookError};
use ruff_python_ast::PySourceType;

use crate::file_revision::FileRevision;
use crate::files::file_root::FileRoots;
use crate::files::private::FileStatus;
use crate::system::{SystemPath, SystemPathBuf, SystemVirtualPath, SystemVirtualPathBuf};
use crate::vendored::{VendoredPath, VendoredPathBuf};
use crate::{vendored, Db, FxDashMap};

mod file_root;
mod path;

/// Interns a file system path and returns a salsa `File` ingredient.
///
/// Returns `Err` if the path doesn't exist, isn't accessible, or if the path points to a directory.
#[inline]
pub fn system_path_to_file(db: &dyn Db, path: impl AsRef<SystemPath>) -> Result<File, FileError> {
    let file = db.files().system(db, path.as_ref());

    // It's important that `vfs.file_system` creates a `VfsFile` even for files that don't exist or don't
    // exist anymore so that Salsa can track that the caller of this function depends on the existence of
    // that file. This function filters out files that don't exist, but Salsa will know that it must
    // re-run the calling query whenever the `file`'s status changes (because of the `.status` call here).
    match file.status(db) {
        FileStatus::Exists => Ok(file),
        FileStatus::IsADirectory => Err(FileError::IsADirectory),
        FileStatus::NotFound => Err(FileError::NotFound),
    }
}

/// Interns a vendored file path. Returns `Some` if the vendored file for `path` exists and `None` otherwise.
#[inline]
pub fn vendored_path_to_file(
    db: &dyn Db,
    path: impl AsRef<VendoredPath>,
) -> Result<File, FileError> {
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

    /// Lookup table that maps [`SystemVirtualPathBuf`]s to [`VirtualFile`] instances.
    system_virtual_by_path: FxDashMap<SystemVirtualPathBuf, VirtualFile>,

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
    fn system(&self, db: &dyn Db, path: &SystemPath) -> File {
        let absolute = SystemPath::absolute(path, db.system().current_directory());

        *self
            .inner
            .system_by_path
            .entry(absolute.clone())
            .or_insert_with(|| {
                tracing::trace!("Adding file '{path}'");

                let metadata = db.system().path_metadata(path);
                let durability = self
                    .root(db, path)
                    .map_or(Durability::default(), |root| root.durability(db));

                let builder = File::builder(FilePath::System(absolute)).durability(durability);

                let builder = match metadata {
                    Ok(metadata) if metadata.file_type().is_file() => builder
                        .permissions(metadata.permissions())
                        .revision(metadata.revision()),
                    Ok(metadata) if metadata.file_type().is_directory() => {
                        builder.status(FileStatus::IsADirectory)
                    }
                    _ => builder.status(FileStatus::NotFound),
                };

                builder.new(db)
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
    fn vendored(&self, db: &dyn Db, path: &VendoredPath) -> Result<File, FileError> {
        let file = match self.inner.vendored_by_path.entry(path.to_path_buf()) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let metadata = match db.vendored().metadata(path) {
                    Ok(metadata) => match metadata.kind() {
                        vendored::FileType::File => metadata,
                        vendored::FileType::Directory => return Err(FileError::IsADirectory),
                    },
                    Err(_) => return Err(FileError::NotFound),
                };

                tracing::trace!("Adding vendored file `{}`", path);
                let file = File::builder(FilePath::Vendored(path.to_path_buf()))
                    .permissions(Some(0o444))
                    .revision(metadata.revision())
                    .durability(Durability::HIGH)
                    .new(db);

                entry.insert(file);

                file
            }
        };

        Ok(file)
    }

    /// Create a new virtual file at the given path and store it for future lookups.
    ///
    /// This will always create a new file, overwriting any existing file at `path` in the internal
    /// storage.
    pub fn virtual_file(&self, db: &dyn Db, path: &SystemVirtualPath) -> VirtualFile {
        tracing::trace!("Adding virtual file {}", path);
        let virtual_file = VirtualFile(
            File::builder(FilePath::SystemVirtual(path.to_path_buf()))
                .status(FileStatus::Exists)
                .revision(FileRevision::zero())
                .permissions(None)
                .new(db),
        );
        self.inner
            .system_virtual_by_path
            .insert(path.to_path_buf(), virtual_file);
        virtual_file
    }

    /// Tries to look up a virtual file by its path. Returns `None` if no such file exists yet.
    pub fn try_virtual_file(&self, path: &SystemVirtualPath) -> Option<VirtualFile> {
        self.inner
            .system_virtual_by_path
            .get(&path.to_path_buf())
            .map(|entry| *entry.value())
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
    pub fn sync_recursively(db: &mut dyn Db, path: &SystemPath) {
        let path = SystemPath::absolute(path, db.system().current_directory());
        tracing::debug!("Syncing all files in '{path}'");

        let inner = Arc::clone(&db.files().inner);
        for entry in inner.system_by_path.iter_mut() {
            if entry.key().starts_with(&path) {
                File::sync_system_path(db, entry.key(), Some(*entry.value()));
            }
        }

        let roots = inner.roots.read().unwrap();

        for root in roots.all() {
            if root.path(db).starts_with(&path) {
                root.set_revision(db).to(FileRevision::now());
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
    pub fn sync_all(db: &mut dyn Db) {
        tracing::debug!("Syncing all files");
        let inner = Arc::clone(&db.files().inner);
        for entry in inner.system_by_path.iter_mut() {
            File::sync_system_path(db, entry.key(), Some(*entry.value()));
        }

        let roots = inner.roots.read().unwrap();

        for root in roots.all() {
            root.set_revision(db).to(FileRevision::now());
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
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

impl std::panic::RefUnwindSafe for Files {}

/// A file that's either stored on the host system's file system or in the vendored file system.
#[salsa::input]
pub struct File {
    /// The path of the file.
    #[return_ref]
    pub path: FilePath,

    /// The unix permissions of the file. Only supported on unix systems. Always `None` on Windows
    /// or when the file has been deleted.
    #[default]
    pub permissions: Option<u32>,

    /// The file revision. A file has changed if the revisions don't compare equal.
    #[default]
    pub revision: FileRevision,

    /// The status of the file.
    ///
    /// Salsa doesn't support deleting inputs. The only way to signal dependent queries that
    /// the file has been deleted is to change the status to `Deleted`.
    #[default]
    status: FileStatus,

    /// Counter that counts the number of created file instances and active file instances.
    /// Only enabled in debug builds.
    #[default]
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
                // Add a dependency on the revision to ensure the operation gets re-executed when the file changes.
                let _ = self.revision(db);

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
                // Add a dependency on the revision to ensure the operation gets re-executed when the file changes.
                let _ = self.revision(db);

                db.system().read_virtual_path_to_notebook(system_virtual)
            }
        }
    }

    /// Refreshes the file metadata by querying the file system if needed.
    pub fn sync_path(db: &mut dyn Db, path: &SystemPath) {
        let absolute = SystemPath::absolute(path, db.system().current_directory());
        Files::touch_root(db, &absolute);
        Self::sync_system_path(db, &absolute, None);
    }

    /// Increments the revision for the virtual file at `path`.
    pub fn sync_virtual_path(db: &mut dyn Db, path: &SystemVirtualPath) {
        if let Some(virtual_file) = db.files().try_virtual_file(path) {
            virtual_file.sync(db);
        }
    }

    /// Syncs the [`File`]'s state with the state of the file on the system.
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
            FilePath::SystemVirtual(_) => {
                VirtualFile(self).sync(db);
            }
        }
    }

    /// Private method providing the implementation for [`Self::sync_path`] and [`Self::sync`] for
    /// system paths.
    fn sync_system_path(db: &mut dyn Db, path: &SystemPath, file: Option<File>) {
        let Some(file) = file.or_else(|| db.files().try_system(db, path)) else {
            return;
        };

        let (status, revision, permission) = match db.system().path_metadata(path) {
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

        if file.status(db) != status {
            tracing::debug!("Updating the status of `{}`", file.path(db));
            file.set_status(db).to(status);
        }

        if file.revision(db) != revision {
            tracing::debug!("Updating the revision of `{}`", file.path(db));
            file.set_revision(db).to(revision);
        }

        if file.permissions(db) != permission {
            tracing::debug!("Updating the permissions of `{}`", file.path(db));
            file.set_permissions(db).to(permission);
        }
    }

    /// Returns `true` if the file exists.
    pub fn exists(self, db: &dyn Db) -> bool {
        self.status(db) == FileStatus::Exists
    }

    /// Returns `true` if the file should be analyzed as a type stub.
    pub fn is_stub(self, db: &dyn Db) -> bool {
        self.path(db)
            .extension()
            .is_some_and(|extension| PySourceType::from_extension(extension).is_stub())
    }
}

/// A virtual file that doesn't exist on the file system.
///
/// This is a wrapper around a [`File`] that provides additional methods to interact with a virtual
/// file.
#[derive(Copy, Clone)]
pub struct VirtualFile(File);

impl VirtualFile {
    /// Returns the underlying [`File`].
    pub fn file(&self) -> File {
        self.0
    }

    /// Increments the revision of the underlying [`File`].
    fn sync(&self, db: &mut dyn Db) {
        let file = self.0;
        tracing::debug!("Updating the revision of `{}`", file.path(db));
        let current_revision = file.revision(db);
        file.set_revision(db)
            .to(FileRevision::new(current_revision.as_u128() + 1));
    }

    /// Closes the virtual file.
    pub fn close(&self, db: &mut dyn Db) {
        tracing::debug!("Closing virtual file `{}`", self.0.path(db));
        self.0.set_status(db).to(FileStatus::NotFound);
    }
}

// The types in here need to be public because they're salsa ingredients but we
// don't want them to be publicly accessible. That's why we put them into a private module.
mod private {
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
    pub enum FileStatus {
        /// The file exists.
        #[default]
        Exists,

        /// The path isn't a file and instead points to a directory.
        IsADirectory,

        /// The path doesn't exist, isn't accessible, or no longer exists.
        NotFound,
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FileError {
    IsADirectory,
    NotFound,
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::IsADirectory => f.write_str("Is a directory"),
            FileError::NotFound => f.write_str("Not found"),
        }
    }
}

impl std::error::Error for FileError {}

#[cfg(test)]
mod tests {
    use crate::file_revision::FileRevision;
    use crate::files::{system_path_to_file, vendored_path_to_file, FileError};
    use crate::system::DbWithTestSystem;
    use crate::tests::TestDb;
    use crate::vendored::VendoredFileSystemBuilder;
    use zip::CompressionMethod;

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

        assert_eq!(test, Err(FileError::NotFound));
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

        let mut vendored_builder = VendoredFileSystemBuilder::new(CompressionMethod::Stored);
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

        assert_eq!(
            vendored_path_to_file(&db, "test.py"),
            Err(FileError::NotFound)
        );
    }
}
