use std::cell::{RefCell, RefMut};
use std::io::{self, Read};
use std::ops::{Deref, DerefMut};
use std::sync::{Mutex, MutexGuard};

use itertools::Itertools;
use zip::{read::ZipFile, ZipArchive};

pub use path::{VendoredPath, VendoredPathBuf};

pub mod path;

type Result<T> = io::Result<T>;

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
        archive.lookup_path(&normalized).is_ok()
            || archive
                .lookup_path(&normalized.with_trailing_slash())
                .is_ok()
    }

    /// Return the crc32 hash of the file
    pub fn file_hash(&self, path: &VendoredPath) -> Option<u32> {
        let normalized = normalize_vendored_path(path);
        let inner_locked = self.inner.lock();
        let mut archive = inner_locked.borrow_mut();
        if let Ok(zip_file) = archive.lookup_path(&normalized) {
            return Some(zip_file.crc32());
        }
        archive
            .lookup_path(&normalized.with_trailing_slash())
            .map(|zip_file| zip_file.crc32())
            .ok()
    }

    /// Read the entire contents of the path into a string
    pub fn read(&self, path: &VendoredPath) -> Option<String> {
        let inner_locked = self.inner.lock();
        let mut archive = inner_locked.borrow_mut();
        let mut zip_file = archive.lookup_path(&normalize_vendored_path(path)).ok()?;
        let mut buffer = String::new();
        zip_file.read_to_string(&mut buffer).ok()?;
        Some(buffer)
    }
}

#[derive(Debug)]
struct VendoredFileSystemInner(Mutex<RefCell<VendoredZipArchive>>);

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
        LockedZipArchive(self.0.lock().unwrap())
    }
}

#[derive(Debug)]
struct LockedZipArchive<'a>(MutexGuard<'a, RefCell<VendoredZipArchive>>);

impl<'a> LockedZipArchive<'a> {
    fn borrow_mut(&self) -> ArchiveReader {
        ArchiveReader(self.0.borrow_mut())
    }
}

#[derive(Debug)]
struct ArchiveReader<'a>(RefMut<'a, VendoredZipArchive>);

impl<'a> Deref for ArchiveReader<'a> {
    type Target = VendoredZipArchive;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> DerefMut for ArchiveReader<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
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
    debug_assert!(
        !path.as_std_path().is_absolute(),
        "Attempted to lookup an absolute filesystem path relative to a vendored zip archive"
    );
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
        assert!(mock_typeshed.file_hash(path).is_some())
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
        assert!(mock_typeshed.file_hash(path).is_none());
        assert!(mock_typeshed.read(path).is_none());
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
        assert!(mock_typeshed.file_hash(path).is_some());
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
