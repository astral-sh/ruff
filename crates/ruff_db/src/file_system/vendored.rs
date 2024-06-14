use std::fmt;
use std::io::{Cursor, Read};
use std::ops::Deref;
use std::sync::Arc;

use itertools::Itertools;
use zip::{read::ZipFile, ZipArchive};

use crate::file_system::{FileRevision, FileSystem, FileSystemPath, FileType, Metadata, Result};

#[derive(Debug, Clone)]
pub struct VendoredFileSystem {
    inner: VendoredZipArchive,
}

impl VendoredFileSystem {
    const READ_ONLY_PERMISSIONS: u32 = 0o444;

    pub fn new(raw_bytes: &'static [u8]) -> Result<Self> {
        Ok(Self {
            inner: VendoredZipArchive::from_static_data(raw_bytes)?,
        })
    }

    /// Alternative, private constructor for testing purposes
    #[cfg(test)]
    fn from_dynamic_data(data: &[u8]) -> Result<Self> {
        Ok(Self {
            inner: VendoredZipArchive::from_dynamic_data(data)?,
        })
    }

    /// Returns an owned copy of the underlying zip archive,
    /// that can be queried for information regarding contained files and directories
    fn vendored_archive(&self) -> VendoredZipArchive {
        // Zip lookups require a mutable reference to the archive,
        // which creates a nightmare with lifetimes unless you own the archive, hence the clone.
        //
        // The zip documentation states that:
        //
        //     At the moment, this type is cheap to clone if this is the case for the reader it uses.
        //     However, this is not guaranteed by this crate and it may change in the future.
        //
        // So this should be fairly cheap *for now*.
        self.inner.clone()
    }

    fn read_zip_file(mut zip_file: ZipFile) -> Result<String> {
        let mut buffer = String::new();
        zip_file.read_to_string(&mut buffer)?;
        Ok(buffer)
    }
}

impl FileSystem for VendoredFileSystem {
    fn exists(&self, path: &FileSystemPath) -> bool {
        let normalized = normalize_vendored_path(path);
        self.vendored_archive().lookup_path(&normalized).is_ok()
            || (!normalized.has_trailing_slash()
                && self
                    .vendored_archive()
                    .lookup_path(&normalized.with_trailing_slash())
                    .is_ok())
    }

    fn is_directory(&self, path: &FileSystemPath) -> bool {
        let normalized = normalize_vendored_path(path);
        if let Ok(zip_file) = self.vendored_archive().lookup_path(&normalized) {
            return zip_file.is_dir();
        }
        if normalized.has_trailing_slash() {
            return false;
        }
        self.vendored_archive()
            .lookup_path(&normalized.with_trailing_slash())
            .is_ok_and(|zip_file| zip_file.is_dir())
    }

    fn is_file(&self, path: &FileSystemPath) -> bool {
        if path.as_str().ends_with('/') {
            return false;
        }
        self.vendored_archive()
            .lookup_path(&normalize_vendored_path(path))
            .is_ok_and(|zip_file| zip_file.is_file())
    }

    fn metadata(&self, path: &FileSystemPath) -> Result<Metadata> {
        let normalized = normalize_vendored_path(path);
        let mut archive = self.vendored_archive();

        let (is_dir, last_modified) = match archive.lookup_path(&normalized) {
            Ok(zip_file) => (zip_file.is_dir(), zip_file.last_modified()),
            Err(err) => {
                if normalized.has_trailing_slash() {
                    return Err(err);
                }
                let mut archive = self.vendored_archive();
                let lookup_result = archive.lookup_path(&normalized.with_trailing_slash());
                if let Ok(zip_file) = lookup_result {
                    (zip_file.is_dir(), zip_file.last_modified())
                } else {
                    return Err(err);
                }
            }
        };

        let file_type = if is_dir {
            FileType::Directory
        } else {
            FileType::File
        };

        let last_modified_timestamp = last_modified
            .to_time()
            .expect("Expected the last-modified time of the file to be representable as an OffsetDateTime")
            .unix_timestamp_nanos();

        let last_modified_timestamp = u128::try_from(last_modified_timestamp)
            .expect("Expected the UTC offset to be a positive number");

        Ok(Metadata {
            revision: FileRevision::new(last_modified_timestamp),
            permissions: Some(Self::READ_ONLY_PERMISSIONS),
            file_type,
        })
    }

    fn read(&self, path: &FileSystemPath) -> Result<String> {
        let normalized = normalize_vendored_path(path);
        let mut archive = self.vendored_archive();
        let lookup_error = match archive.lookup_path(&normalized) {
            Ok(zip_file) => return Self::read_zip_file(zip_file),
            Err(err) => err,
        };
        if normalized.has_trailing_slash() {
            return Err(lookup_error);
        }
        let zip_file = archive.lookup_path(&normalized.with_trailing_slash())?;
        Self::read_zip_file(zip_file)
    }
}

#[derive(Clone)]
enum VendoredZipData {
    /// Used for zips that are available at build time
    Static(&'static [u8]),

    /// Used mainly for testing: we generally want to create mock zip archives
    /// inside the test, so that we can easily see what's in the archive.
    /// This means that it's hard for the mock zip archive to be available at build time.
    #[allow(unused)]
    Dynamic(Arc<[u8]>),
}

impl Deref for VendoredZipData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Static(data) => data,
            Self::Dynamic(data) => data,
        }
    }
}

impl AsRef<[u8]> for VendoredZipData {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl fmt::Debug for VendoredZipData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<Binary data of length {}>", self.len())
    }
}

/// Newtype wrapper around a ZipArchive.
///
/// Since ZipArchives are generally cheap to clone (for now), this should be too.
#[derive(Debug, Clone)]
struct VendoredZipArchive(ZipArchive<Cursor<VendoredZipData>>);

impl VendoredZipArchive {
    fn from_static_data(data: &'static [u8]) -> Result<Self> {
        Ok(Self(ZipArchive::new(Cursor::new(
            VendoredZipData::Static(data),
        ))?))
    }

    #[cfg(test)]
    fn from_dynamic_data(data: &[u8]) -> Result<Self> {
        Ok(Self(ZipArchive::new(Cursor::new(
            VendoredZipData::Dynamic(Arc::from(data)),
        ))?))
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
        self.0.push('/');
        self
    }

    fn has_trailing_slash(&self) -> bool {
        self.0.ends_with('/')
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
fn normalize_vendored_path(path: &FileSystemPath) -> NormalizedVendoredPath {
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

    let mut normalized = normalized_parts.into_iter().join("/");
    // Restore the trailing slash if there was one (camino normalizes these away),
    // as this is how zip archives distinguish between files and directories.
    if path.as_str().ends_with('/') {
        normalized.push('/');
    }

    NormalizedVendoredPath(normalized)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use once_cell::sync::Lazy;
    use zip::{write::FileOptions, CompressionMethod, ZipWriter};

    use super::*;

    const FUNCTOOLS_CONTENTS: &str = "def update_wrapper(): ...";
    const ASYNCIO_TASKS_CONTENTS: &str = "class Task: ...";

    static MOCK_ZIP_ARCHIVE: Lazy<Arc<[u8]>> = Lazy::new(|| {
        let mut typeshed_buffer = Vec::new();
        let typeshed = Cursor::new(&mut typeshed_buffer);

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

        Arc::from(typeshed_buffer.into_boxed_slice())
    });

    fn mock_typeshed() -> VendoredFileSystem {
        VendoredFileSystem::from_dynamic_data(&MOCK_ZIP_ARCHIVE).unwrap()
    }

    fn test_directory(dirname: &str) {
        let mock_typeshed = mock_typeshed();

        let path = FileSystemPath::new(dirname);

        assert!(mock_typeshed.exists(path));
        assert!(mock_typeshed.is_directory(path));
        assert!(!mock_typeshed.is_file(path));

        let path_metadata = mock_typeshed.metadata(path).unwrap();
        assert!(path_metadata.file_type().is_directory());
        assert!(path_metadata.permissions().is_some());
        path_metadata.revision();
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
        let path = FileSystemPath::new(path);
        assert!(!mock_typeshed.exists(path));
        assert!(!mock_typeshed.is_directory(path));
        assert!(!mock_typeshed.is_file(path));
        assert!(mock_typeshed.metadata(path).is_err());
        assert!(mock_typeshed.read(path).is_err());
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

    fn test_file(mock_typeshed: &VendoredFileSystem, path: &FileSystemPath) {
        assert!(mock_typeshed.exists(path));
        assert!(mock_typeshed.is_file(path));
        assert!(!mock_typeshed.is_directory(path));

        let path_metadata = mock_typeshed.metadata(path).unwrap();
        assert!(path_metadata.file_type().is_file());
        assert!(path_metadata.permissions().is_some());
        path_metadata.revision();
    }

    #[test]
    fn functools_file_contents() {
        let mock_typeshed = mock_typeshed();
        let path = FileSystemPath::new("stdlib/functools.pyi");
        test_file(&mock_typeshed, path);
        let functools_stub = mock_typeshed.read(path).unwrap();
        assert_eq!(functools_stub.as_str(), FUNCTOOLS_CONTENTS);
    }

    #[test]
    fn functools_file_other_path() {
        test_file(
            &mock_typeshed(),
            FileSystemPath::new("stdlib/../stdlib/../stdlib/functools.pyi"),
        )
    }

    #[test]
    fn asyncio_file_contents() {
        let mock_typeshed = mock_typeshed();
        let path = FileSystemPath::new("stdlib/asyncio/tasks.pyi");
        test_file(&mock_typeshed, path);
        let asyncio_stub = mock_typeshed.read(path).unwrap();
        assert_eq!(asyncio_stub.as_str(), ASYNCIO_TASKS_CONTENTS);
    }

    #[test]
    fn asyncio_file_other_path() {
        test_file(
            &mock_typeshed(),
            FileSystemPath::new("./stdlib/asyncio/../asyncio/tasks.pyi"),
        )
    }
}
