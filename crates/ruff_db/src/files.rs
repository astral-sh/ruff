use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dashmap::mapref::entry::Entry;
pub use directory::{
    DirectoryListing, DirectoryListingError, directory_listing, system_path_to_directory,
};
pub use file_root::{FileRoot, FileRootKind};
pub use path::FilePath;
use ruff_notebook::{Notebook, NotebookError};
use ruff_python_ast::PySourceType;
use ruff_text_size::{Ranged, TextRange};
use salsa::plumbing::AsId;
use salsa::{Durability, Setter};

use crate::diagnostic::{Span, UnifiedFile};
use crate::file_revision::FileRevision;
use crate::files::file_root::FileRoots;
use crate::files::private::FileStatus;
use crate::source::SourceText;
use crate::system::{
    SystemPath, SystemPathBuf, SystemVirtualPath, SystemVirtualPathBuf, deduplicate_nested_paths,
};
use crate::vendored::{VendoredPath, VendoredPathBuf};
use crate::{Db, FxDashMap, vendored};

mod directory;
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
#[derive(Default, Clone)]
pub struct Files {
    inner: Arc<FilesInner>,
}

#[derive(Default)]
struct FilesInner {
    /// Whether inputs on newly created files should be frozen.
    frozen: AtomicBool,

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
    /// Freezes all inputs on files created from now on.
    ///
    /// Existing files retain their current durability. Callers should therefore call this before
    /// discovering any files if they need the freeze to apply to the entire project.
    pub fn freeze(&self) {
        self.inner.frozen.store(true, Ordering::Relaxed);
    }

    fn input_durability(&self, default: Durability) -> Durability {
        if self.inner.frozen.load(Ordering::Relaxed) {
            Durability::NEVER_CHANGE
        } else {
            default
        }
    }

    /// Looks up a file by its `path`.
    ///
    /// For a non-existing file, creates a new salsa [`File`] ingredient and stores it for future lookups.
    ///
    /// The operation always succeeds even if the path doesn't exist on disk, isn't accessible or if the path points to a directory.
    /// In these cases, a file with the appropriate [`FileStatus`] is returned.
    fn system(&self, db: &dyn Db, path: &SystemPath) -> File {
        let absolute = SystemPath::absolute(path, db.system().current_directory());

        // DashMap's entry API requires an owned key. Avoid cloning it for cached paths.
        if let Some(file) = self.inner.system_by_path.get(absolute.as_path()) {
            return *file;
        }

        *self
            .inner
            .system_by_path
            .entry(absolute.clone())
            .or_insert_with(|| {
                let metadata = db.system().path_metadata(path);

                tracing::trace!("Adding file '{absolute}'");

                let durability = self.input_durability(
                    self.root(db, &absolute)
                        .map_or(Durability::default(), |root| root.durability(db)),
                );

                let builder = File::builder(FilePath::from(absolute))
                    .durability(durability)
                    .path_durability(Durability::NEVER_CHANGE);

                let builder = match metadata {
                    Ok(metadata) if metadata.file_type().is_file() => builder
                        .permissions(metadata.permissions())
                        .revision(metadata.revision()),
                    Ok(metadata) if metadata.file_type().is_directory() => builder
                        .durability(Durability::MEDIUM.max(durability))
                        .status(FileStatus::IsADirectory)
                        .permissions(metadata.permissions())
                        .revision(metadata.revision()),
                    _ => builder
                        .status(FileStatus::NotFound)
                        .status_durability(Durability::MEDIUM.max(durability)),
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
        if let Some(file) = self.inner.vendored_by_path.get(path) {
            return Ok(*file);
        }

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
                let file = File::builder(FilePath::from(path))
                    .permissions(Some(0o444))
                    .revision(metadata.revision())
                    .durability(Durability::NEVER_CHANGE)
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
            File::builder(FilePath::from(path))
                .durability(self.input_durability(Durability::LOW))
                .path_durability(Durability::NEVER_CHANGE)
                .status(FileStatus::Exists)
                .revision(FileRevision::zero())
                .permissions(None)
                .permissions_durability(Durability::NEVER_CHANGE)
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
            .get(path)
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

    /// Refreshes the state of all known files under `paths` recursively.
    ///
    /// The most common use case is to update the [`Files`] state after removing or moving directories.
    ///
    /// # Performance
    /// Refreshing the state of files recursively is expensive. It requires iterating over all known files
    /// and making system calls to get the latest status of matching files.
    /// That's why [`File::sync_path`] is preferred if it is known that the path is a file.
    pub fn sync_all_recursive<P, I>(db: &mut dyn Db, paths: I)
    where
        P: AsRef<SystemPath>,
        I: IntoIterator<Item = P>,
    {
        let current_directory = db.system().current_directory();
        let paths = deduplicate_nested_paths(
            paths
                .into_iter()
                .map(|path| SystemPath::absolute(path.as_ref(), current_directory)),
        )
        .collect::<BTreeSet<_>>();

        if paths.is_empty() {
            return;
        }

        let parents = paths
            .iter()
            .filter_map(|path| path.parent().map(SystemPath::to_path_buf))
            .collect::<BTreeSet<_>>();

        let inner = Arc::clone(&db.files().inner);
        for entry in inner.system_by_path.iter_mut() {
            let path = entry.key();
            if paths
                .range(..=path.to_path_buf())
                .next_back()
                .is_some_and(|candidate| path.starts_with(candidate.as_path()))
                || parents.contains(path)
            {
                File::sync_system_path(db, path, Some(*entry.value()));
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
    }
}

impl fmt::Debug for Files {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            let mut map = f.debug_map();

            for entry in self.inner.system_by_path.iter() {
                map.entry(entry.key(), entry.value());
            }
            map.finish()
        } else {
            f.debug_struct("Files")
                .field("system_by_path", &self.inner.system_by_path.len())
                .field(
                    "system_virtual_by_path",
                    &self.inner.system_virtual_by_path.len(),
                )
                .field("vendored_by_path", &self.inner.vendored_by_path.len())
                .finish()
        }
    }
}

impl std::panic::RefUnwindSafe for Files {}

/// A file-system path that's either stored on the host system's file system or in the vendored file system.
///
/// # Ordering
/// Ordering is based on the file's salsa-assigned id and not on its values.
/// The id may change between runs.
#[salsa::input(heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct File {
    /// The path of the file (immutable).
    #[returns(ref)]
    pub path: FilePath,

    /// The unix permissions of the file. Only supported on unix systems. Always `None` on Windows
    /// or when the file has been deleted.
    #[default]
    #[returns(copy)]
    pub permissions: Option<u32>,

    /// The path revision. A file or directory has changed if the revisions don't compare equal.
    #[default]
    #[returns(copy)]
    pub revision: FileRevision,

    /// The status of the file.
    ///
    /// Salsa doesn't support deleting inputs. The only way to signal dependent queries that
    /// the file has been deleted is to change the status to `Deleted`.
    #[default]
    #[returns(copy)]
    pub status: FileStatus,

    /// Overrides the result of [`source_text`](crate::source::source_text).
    ///
    /// This is useful when running queries after modifying a file's content but
    /// before the content is written to disk. For example, to verify that the applied fixes
    /// didn't introduce any new errors.
    ///
    /// The override gets automatically removed the next time the file changes.
    #[default]
    #[returns(ref)]
    pub source_text_override: Option<SourceText>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for File {}

struct SyncPathResult {
    status_changed: bool,
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
    ///
    /// Directory listings are invalidated if the path's file status changed, its prior status is
    /// unknown, or if `path` is itself a directory.
    pub fn sync_path(db: &mut dyn Db, path: &SystemPath) {
        let absolute = SystemPath::absolute(path, db.system().current_directory());
        let result = Self::sync_system_path(db, &absolute, None);
        Self::touch_parent_directory_after_sync(db, &absolute, result);
    }

    /// Refreshes *only* the file metadata by querying the file system if needed.
    ///
    /// This specifically does not invalidate any directory listings.
    pub fn sync_path_only(db: &mut dyn Db, path: &SystemPath) {
        let absolute = SystemPath::absolute(path, db.system().current_directory());
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
                let result = Self::sync_system_path(db, &system, Some(self));
                Self::touch_parent_directory_after_sync(db, &system, result);
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
    fn sync_system_path(db: &mut dyn Db, path: &SystemPath, file: Option<File>) -> SyncPathResult {
        let Some(file) = file.or_else(|| db.files().try_system(db, path)) else {
            return SyncPathResult {
                status_changed: true,
            };
        };

        let (status, revision, permission) = match db.system().path_metadata(path) {
            Ok(metadata) if metadata.file_type().is_file() => (
                FileStatus::Exists,
                metadata.revision(),
                metadata.permissions(),
            ),
            Ok(metadata) if metadata.file_type().is_directory() => (
                FileStatus::IsADirectory,
                metadata.revision(),
                metadata.permissions(),
            ),
            _ => (FileStatus::NotFound, FileRevision::zero(), None),
        };

        let mut clear_override = false;

        let old_status = file.status(db);
        let status_changed = old_status != status;

        if status_changed {
            tracing::debug!("Updating the status of `{}`", file.path(db));
            file.set_status(db).to(status);
            clear_override = true;
        }

        if file.revision(db) != revision {
            tracing::debug!("Updating the revision of `{}`", file.path(db));
            file.set_revision(db).to(revision);
            clear_override = true;
        }

        if file.permissions(db) != permission {
            tracing::debug!("Updating the permissions of `{}`", file.path(db));
            file.set_permissions(db).to(permission);
        }

        if clear_override && file.source_text_override(db).is_some() {
            file.set_source_text_override(db).to(None);
        }

        SyncPathResult { status_changed }
    }

    fn touch_parent_directory_after_sync(
        db: &mut dyn Db,
        path: &SystemPath,
        result: SyncPathResult,
    ) {
        if result.status_changed
            && let Some(parent) = path.parent()
        {
            Self::sync_system_path(db, parent, None);
        }
    }

    /// Returns `true` if the file exists.
    pub fn exists(self, db: &dyn Db) -> bool {
        self.status(db) == FileStatus::Exists
    }

    /// Returns `true` if the file should be analyzed as a type stub.
    pub fn is_stub(self, db: &dyn Db) -> bool {
        self.source_type(db).is_stub()
    }

    /// Returns `true` if the file is an `__init__.pyi`
    pub fn is_package_stub(self, db: &dyn Db) -> bool {
        self.path(db).as_str().ends_with("__init__.pyi")
    }

    /// Returns `true` if the file is an `__init__.pyi`
    pub fn is_package(self, db: &dyn Db) -> bool {
        let path = self.path(db).as_str();
        path.ends_with("__init__.pyi") || path.ends_with("__init__.py")
    }

    pub fn source_type(self, db: &dyn Db) -> PySourceType {
        match self.path(db) {
            FilePath::System(path) => path
                .extension()
                .map_or(PySourceType::Python, PySourceType::from_extension),
            FilePath::Vendored(_) => PySourceType::Stub,
            FilePath::SystemVirtual(path) => path
                .extension()
                .map_or(PySourceType::Python, PySourceType::from_extension),
        }
    }
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        salsa::with_attached_database(|db| {
            if f.alternate() {
                f.debug_struct("File")
                    .field("path", &self.path(db))
                    .field("status", &self.status(db))
                    .field("permissions", &self.permissions(db))
                    .field("revision", &self.revision(db))
                    .finish()
            } else {
                f.debug_tuple("File").field(&self.path(db)).finish()
            }
        })
        .unwrap_or_else(|| f.debug_tuple("file").field(&self.as_id()).finish())
    }
}

/// A virtual file that doesn't exist on the file system.
///
/// This is a wrapper around a [`File`] that provides additional methods to interact with a virtual
/// file.
#[derive(Copy, Clone, Debug)]
pub struct VirtualFile(File);

impl VirtualFile {
    /// Returns the underlying [`File`].
    pub fn file(&self) -> File {
        self.0
    }

    /// Increments the revision of the underlying [`File`].
    pub fn sync(&self, db: &mut dyn Db) {
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
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Default, get_size2::GetSize)]
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

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::IsADirectory => f.write_str("Is a directory"),
            FileError::NotFound => f.write_str("Not found"),
        }
    }
}

impl std::error::Error for FileError {}

/// Range with its corresponding file.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct FileRange {
    file: File,
    range: TextRange,
}

impl FileRange {
    pub const fn new(file: File, range: TextRange) -> Self {
        Self { file, range }
    }

    pub const fn file(&self) -> File {
        self.file
    }
}

impl Ranged for FileRange {
    #[inline]
    fn range(&self) -> TextRange {
        self.range
    }
}

impl TryFrom<&Span> for FileRange {
    type Error = ();

    fn try_from(value: &Span) -> Result<Self, Self::Error> {
        let UnifiedFile::Ty(file) = value.file() else {
            return Err(());
        };

        Ok(Self {
            file: *file,
            range: value.range().ok_or(())?,
        })
    }
}

impl TryFrom<Span> for FileRange {
    type Error = ();

    fn try_from(value: Span) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

#[cfg(test)]
mod tests {
    use salsa::Setter;

    use crate::Db as _;
    use crate::file_revision::FileRevision;
    use crate::files::{FileError, system_path_to_file, vendored_path_to_file};
    use crate::source::source_text;
    use crate::system::DbWithWritableSystem as _;
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
    #[should_panic]
    fn freeze_applies_to_new_file_overrides() {
        let mut db = TestDb::new();
        db.write_file("test.py", "x = 1").unwrap();
        db.files().freeze();

        let file = system_path_to_file(&db, "test.py").unwrap();
        let source = source_text(&db, file);
        file.set_source_text_override(&mut db).to(Some(source));
    }

    #[test]
    fn freeze_does_not_change_existing_files() {
        let mut db = TestDb::new();
        db.write_file("test.py", "x = 1").unwrap();

        let file = system_path_to_file(&db, "test.py").unwrap();
        db.files().freeze();

        let source = source_text(&db, file);
        file.set_source_text_override(&mut db)
            .to(Some(source.clone()));
        assert_eq!(file.source_text_override(&db).as_ref(), Some(&source));
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
