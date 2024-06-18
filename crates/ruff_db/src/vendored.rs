use std::cell::RefCell;
use std::io::{self, Read};
use std::sync::{Mutex, MutexGuard};

use itertools::Itertools;
use zip::{read::ZipFile, ZipArchive};

use crate::file_revision::FileRevision;
pub use path::{VendoredPath, VendoredPathBuf};

pub mod path;

type Result<T> = io::Result<T>;

/// File system that stores all content in a static zip archive
/// bundled as part of the Ruff binary.
///
/// "Files" in the `VendoredFileSystem` are read-only and immutable.
/// Directories are supported, but symlinks and hardlinks cannot exist.
#[derive(Debug)]
pub struct VendoredFileSystem {
    inner: VendoredFileSystemInner,
}

impl VendoredFileSystem {
    pub fn new(raw_bytes: &'static [u8]) -> Result<Self> {
        Ok(Self {
            inner: VendoredFileSystemInner::new(raw_bytes)?,
        })
    }

    pub fn exists(&self, path: &VendoredPath) -> bool {
        let normalized = normalize_vendored_path(path);
        let inner_locked = self.inner.lock();
        let mut archive = inner_locked.borrow_mut();

        // Must probe the zipfile twice, as "stdlib" and "stdlib/" are considered
        // different paths in a zip file, but we want to abstract over that difference here
        // so that paths relative to the `VendoredFileSystem`
        // work the same as other paths in Ruff.
        archive.lookup_path(&normalized).is_ok()
            || archive
                .lookup_path(&normalized.with_trailing_slash())
                .is_ok()
    }

    pub fn metadata(&self, path: &VendoredPath) -> Option<Metadata> {
        let normalized = normalize_vendored_path(path);
        let inner_locked = self.inner.lock();

        // Must probe the zipfile twice, as "stdlib" and "stdlib/" are considered
        // different paths in a zip file, but we want to abstract over that difference here
        // so that paths relative to the `VendoredFileSystem`
        // work the same as other paths in Ruff.
        let mut archive = inner_locked.borrow_mut();
        if let Ok(zip_file) = archive.lookup_path(&normalized) {
            return Some(Metadata::from_zip_file(zip_file));
        }
        if let Ok(zip_file) = archive.lookup_path(&normalized.with_trailing_slash()) {
            return Some(Metadata::from_zip_file(zip_file));
        }

        None
    }

    /// Read the entire contents of the zip file at `path` into a string
    ///
    /// Returns an Err() if any of the following are true:
    /// - The path does not exist in the underlying zip archive
    /// - The path exists in the underlying zip archive, but represents a directory
    /// - The contents of the zip file at `path` contain invalid UTF-8
    pub fn read(&self, path: &VendoredPath) -> Result<String> {
        let inner_locked = self.inner.lock();
        let mut archive = inner_locked.borrow_mut();
        let mut zip_file = archive.lookup_path(&normalize_vendored_path(path))?;
        let mut buffer = String::new();
        zip_file.read_to_string(&mut buffer)?;
        Ok(buffer)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileType {
    /// The path exists in the zip archive and represents a vendored file
    File,

    /// The path exists in the zip archive and represents a vendored directory of files
    Directory,
}

impl FileType {
    pub const fn is_file(self) -> bool {
        matches!(self, Self::File)
    }

    pub const fn is_directory(self) -> bool {
        matches!(self, Self::Directory)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata {
    kind: FileType,
    revision: FileRevision,
}

impl Metadata {
    fn from_zip_file(zip_file: ZipFile) -> Self {
        let kind = if zip_file.is_dir() {
            FileType::Directory
        } else {
            FileType::File
        };

        Self {
            kind,
            revision: FileRevision::new(u128::from(zip_file.crc32())),
        }
    }

    pub fn kind(&self) -> FileType {
        self.kind
    }

    pub fn revision(&self) -> FileRevision {
        self.revision
    }
}

#[derive(Debug)]
struct VendoredFileSystemInner(Mutex<RefCell<VendoredZipArchive>>);

type LockedZipArchive<'a> = MutexGuard<'a, RefCell<VendoredZipArchive>>;

impl VendoredFileSystemInner {
    fn new(raw_bytes: &'static [u8]) -> Result<Self> {
        Ok(Self(Mutex::new(RefCell::new(VendoredZipArchive::new(
            raw_bytes,
        )?))))
    }

    /// Acquire a lock on the underlying zip archive.
    /// The call will block until it is able to acquire the lock.
    ///
    /// ## Panics:
    /// If the current thread already holds the lock.
    fn lock(&self) -> LockedZipArchive {
        self.0.lock().unwrap()
    }
}

/// Newtype wrapper around a ZipArchive.
#[derive(Debug)]
struct VendoredZipArchive(ZipArchive<io::Cursor<&'static [u8]>>);

impl VendoredZipArchive {
    fn new(data: &'static [u8]) -> Result<Self> {
        Ok(Self(ZipArchive::new(io::Cursor::new(data))?))
    }

    fn lookup_path(&mut self, path: &NormalizedVendoredPath) -> Result<ZipFile> {
        Ok(self.0.by_name(path.as_str())?)
    }
}

/// A path that has been normalized via the `normalize_vendored_path` function.
///
/// Trailing slashes are normalized away by `camino::Utf8PathBuf`s,
/// but trailing slashes are crucial for distinguishing between
/// files and directories inside zip archives.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedVendoredPath(String);

impl NormalizedVendoredPath {
    fn with_trailing_slash(mut self) -> Self {
        debug_assert!(!self.0.ends_with('/'));
        self.0.push('/');
        self
    }

    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Normalizes the path by removing `.` and `..` components.
///
/// ## Panics:
/// If a path with an unsupported component for vendored paths is passed.
/// Unsupported components are path prefixes,
/// and path root directories appearing anywhere except at the start of the path.
fn normalize_vendored_path(path: &VendoredPath) -> NormalizedVendoredPath {
    let mut normalized_parts = camino::Utf8PathBuf::new();

    // Allow the `RootDir` component, but only if it is at the very start of the string.
    let mut components = path.components().peekable();
    if let Some(camino::Utf8Component::RootDir) = components.peek() {
        components.next();
    }

    for component in components {
        match component {
            camino::Utf8Component::Normal(part) => normalized_parts.push(part),
            camino::Utf8Component::CurDir => continue,
            camino::Utf8Component::ParentDir => {
                normalized_parts.pop();
            }
            unsupported => panic!("Unsupported component in a vendored path: {unsupported}"),
        }
    }
    NormalizedVendoredPath(normalized_parts.into_iter().join("/"))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use once_cell::sync::Lazy;
    use zip::{write::FileOptions, CompressionMethod, ZipWriter};

    use super::*;

    const FUNCTOOLS_CONTENTS: &str = "def update_wrapper(): ...";
    const ASYNCIO_TASKS_CONTENTS: &str = "class Task: ...";

    static MOCK_ZIP_ARCHIVE: Lazy<Box<[u8]>> = Lazy::new(|| {
        let mut typeshed_buffer = Vec::new();
        let typeshed = io::Cursor::new(&mut typeshed_buffer);

        let options = FileOptions::default()
            .compression_method(CompressionMethod::Zstd)
            .unix_permissions(0o644);

        {
            let mut archive = ZipWriter::new(typeshed);

            archive.add_directory("stdlib/", options).unwrap();
            archive.start_file("stdlib/functools.pyi", options).unwrap();
            archive.write_all(FUNCTOOLS_CONTENTS.as_bytes()).unwrap();

            archive.add_directory("stdlib/asyncio/", options).unwrap();
            archive
                .start_file("stdlib/asyncio/tasks.pyi", options)
                .unwrap();
            archive
                .write_all(ASYNCIO_TASKS_CONTENTS.as_bytes())
                .unwrap();

            archive.finish().unwrap();
        }

        typeshed_buffer.into_boxed_slice()
    });

    fn mock_typeshed() -> VendoredFileSystem {
        VendoredFileSystem::new(&MOCK_ZIP_ARCHIVE).unwrap()
    }

    fn test_directory(dirname: &str) {
        let mock_typeshed = mock_typeshed();

        let path = VendoredPath::new(dirname);

        assert!(mock_typeshed.exists(path));
        assert!(mock_typeshed.read(path).is_err());
        let metadata = mock_typeshed.metadata(path).unwrap();
        assert!(metadata.kind.is_directory());
    }

    #[test]
    fn stdlib_dir_no_trailing_slash() {
        test_directory("stdlib")
    }

    #[test]
    fn stdlib_dir_trailing_slash() {
        test_directory("stdlib/")
    }

    #[test]
    fn asyncio_dir_no_trailing_slash() {
        test_directory("stdlib/asyncio")
    }

    #[test]
    fn asyncio_dir_trailing_slash() {
        test_directory("stdlib/asyncio/")
    }

    #[test]
    fn stdlib_dir_parent_components() {
        test_directory("stdlib/asyncio/../../stdlib")
    }

    #[test]
    fn asyncio_dir_odd_components() {
        test_directory("./stdlib/asyncio/../asyncio/")
    }

    fn test_nonexistent_path(path: &str) {
        let mock_typeshed = mock_typeshed();
        let path = VendoredPath::new(path);
        assert!(!mock_typeshed.exists(path));
        assert!(mock_typeshed.metadata(path).is_none());
        assert!(mock_typeshed
            .read(path)
            .is_err_and(|err| err.to_string().contains("file not found")));
    }

    #[test]
    fn simple_nonexistent_path() {
        test_nonexistent_path("foo")
    }

    #[test]
    fn nonexistent_path_with_extension() {
        test_nonexistent_path("foo.pyi")
    }

    #[test]
    fn nonexistent_path_with_trailing_slash() {
        test_nonexistent_path("foo/")
    }

    #[test]
    fn nonexistent_path_with_fancy_components() {
        test_nonexistent_path("./foo/../../../foo")
    }

    fn test_file(mock_typeshed: &VendoredFileSystem, path: &VendoredPath) {
        assert!(mock_typeshed.exists(path));
        let metadata = mock_typeshed.metadata(path).unwrap();
        assert!(metadata.kind.is_file());
    }

    #[test]
    fn functools_file_contents() {
        let mock_typeshed = mock_typeshed();
        let path = VendoredPath::new("stdlib/functools.pyi");
        test_file(&mock_typeshed, path);
        let functools_stub = mock_typeshed.read(path).unwrap();
        assert_eq!(functools_stub.as_str(), FUNCTOOLS_CONTENTS);
        // Test that using the RefCell doesn't mutate
        // the internal state of the underlying zip archive incorrectly:
        let functools_stub_again = mock_typeshed.read(path).unwrap();
        assert_eq!(functools_stub_again.as_str(), FUNCTOOLS_CONTENTS);
    }

    #[test]
    fn functools_file_other_path() {
        test_file(
            &mock_typeshed(),
            VendoredPath::new("stdlib/../stdlib/../stdlib/functools.pyi"),
        )
    }

    #[test]
    fn asyncio_file_contents() {
        let mock_typeshed = mock_typeshed();
        let path = VendoredPath::new("stdlib/asyncio/tasks.pyi");
        test_file(&mock_typeshed, path);
        let asyncio_stub = mock_typeshed.read(path).unwrap();
        assert_eq!(asyncio_stub.as_str(), ASYNCIO_TASKS_CONTENTS);
    }

    #[test]
    fn asyncio_file_other_path() {
        test_file(
            &mock_typeshed(),
            VendoredPath::new("./stdlib/asyncio/../asyncio/tasks.pyi"),
        )
    }
}
