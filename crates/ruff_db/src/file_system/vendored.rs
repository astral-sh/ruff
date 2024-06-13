use std::io::{Cursor, Read};
use std::sync::Arc;

use itertools::Itertools;
use zip::{read::ZipFile, result::ZipResult, ZipArchive};

use crate::file_system::{FileRevision, FileSystem, FileSystemPath, FileType, Metadata, Result};

#[derive(Debug, Clone)]
pub struct VendoredFileSystem {
    inner: VendoredZipArchive,
}

impl VendoredFileSystem {
    const READ_ONLY_PERMISSIONS: u32 = 0o444;

    pub fn new(raw_bytes: Arc<[u8]>) -> Result<Self> {
        Ok(Self {
            inner: VendoredZipArchive::new(raw_bytes)?,
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

    /// Create a new empty string, read the contents of `zipfile` into it, and return the string
    fn read_zipfile(mut zipfile: ZipFile) -> Result<String> {
        let mut buffer = String::with_capacity(zipfile.size() as usize);
        zipfile.read_to_string(&mut buffer)?;
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
        if let Ok(zipfile) = self.vendored_archive().lookup_path(&normalized) {
            return zipfile.is_dir();
        }
        if normalized.has_trailing_slash() {
            return false;
        }
        self.vendored_archive()
            .lookup_path(&normalized.with_trailing_slash())
            .is_ok_and(|zipfile| zipfile.is_dir())
    }

    fn is_file(&self, path: &FileSystemPath) -> bool {
        if path.as_str().ends_with('/') {
            return false;
        }
        self.vendored_archive()
            .lookup_path(&normalize_vendored_path(path))
            .is_ok_and(|zipfile| zipfile.is_file())
    }

    fn metadata(&self, path: &FileSystemPath) -> Result<Metadata> {
        let normalized = normalize_vendored_path(path);
        let mut archive = self.vendored_archive();

        let is_dir = match archive.lookup_path(&normalized) {
            Ok(zipfile) => zipfile.is_dir(),
            Err(err) => {
                if normalized.has_trailing_slash() {
                    return Err(err);
                }
                let mut archive = self.vendored_archive();
                let lookup_result = archive.lookup_path(&normalized.with_trailing_slash());
                if let Ok(zipfile) = lookup_result {
                    zipfile.is_dir()
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

        Ok(Metadata {
            revision: FileRevision::zero(),
            permissions: Some(Self::READ_ONLY_PERMISSIONS),
            file_type,
        })
    }

    fn read(&self, path: &FileSystemPath) -> Result<String> {
        let normalized = normalize_vendored_path(path);
        let mut archive = self.vendored_archive();
        let lookup_error = match archive.lookup_path(&normalized) {
            Ok(zipfile) => return Self::read_zipfile(zipfile),
            Err(err) => err,
        };
        if normalized.has_trailing_slash() {
            return Err(lookup_error);
        }
        let mut archive = self.vendored_archive();
        let zipfile = archive.lookup_path(&normalized.with_trailing_slash())?;
        Self::read_zipfile(zipfile)
    }
}

/// Newtype wrapper around a ZipArchive.
///
/// Since ZipArchives are generally cheap to clone (for now), this should be too.
#[derive(Debug, Clone)]
struct VendoredZipArchive(ZipArchive<Cursor<Arc<[u8]>>>);

impl VendoredZipArchive {
    fn new(vendored_bytes: Arc<[u8]>) -> ZipResult<Self> {
        Ok(Self(ZipArchive::new(Cursor::new(vendored_bytes))?))
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
    use std::fs;
    use std::io::Write;

    use once_cell::sync::Lazy;
    use tempfile::TempDir;
    use zip::{write::FileOptions, CompressionMethod, ZipWriter};

    use super::*;

    const FUNCTOOLS_CONTENTS: &str = "def update_wrapper(): ...";
    const ASYNCIO_TASKS_CONTENTS: &str = "class Task: ...";

    static MOCK_ZIP_ARCHIVE: Lazy<Arc<[u8]>> = Lazy::new(|| {
        let td = TempDir::new().unwrap();
        let typeshed_path = td.path().join("typeshed");
        let typeshed = fs::File::create(&typeshed_path).unwrap();

        let options = FileOptions::default()
            .compression_method(CompressionMethod::Zstd)
            .unix_permissions(0o644);

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
        Arc::from(fs::read(typeshed_path).unwrap().into_boxed_slice())
    });

    fn mock_typeshed() -> VendoredFileSystem {
        VendoredFileSystem::new(Arc::clone(&MOCK_ZIP_ARCHIVE)).unwrap()
    }

    macro_rules! directory_tests {
        ($($name:ident: $dirname:literal,)*) => {
        $(
            #[test]
            fn $name() {
                let mock_typeshed = mock_typeshed();

                let path = FileSystemPath::new($dirname);

                assert!(mock_typeshed.exists(path));
                assert!(mock_typeshed.is_directory(path));
                assert!(!mock_typeshed.is_file(path));

                let path_metadata = mock_typeshed.metadata(path).unwrap();
                assert!(path_metadata.file_type().is_directory());
                assert!(path_metadata.permissions().is_some());
                assert_eq!(path_metadata.revision(), FileRevision::zero());
            }
        )*
        }
    }

    directory_tests! {
        stdlib_dir_no_trailing_slash: "stdlib",
        stdlib_dir_trailing_slash: "stdlib/",
        asyncio_dir_no_trailing_slash: "stdlib/asyncio",
        asyncio_dir_trailing_slash: "stdlib/asyncio/",
        stdlib_dir_parent_components: "stdlib/asyncio/../../stdlib",
        asyncio_dir_odd_components: "./stdlib/asyncio/../asyncio/",
    }

    macro_rules! nonexistent_path_tests {
        ($($name:ident: $path:literal,)*) => {
        $(
            #[test]
            fn $name() {
                let mock_typeshed = mock_typeshed();
                let path = FileSystemPath::new($path);
                assert!(!mock_typeshed.exists(path));
                assert!(!mock_typeshed.is_directory(path));
                assert!(!mock_typeshed.is_file(path));
                assert!(mock_typeshed.metadata(path).is_err());
                assert!(mock_typeshed.read(path).is_err());
            }
        )*
        }
    }

    nonexistent_path_tests! {
        simple_path: "foo",
        path_with_extension: "foo.pyi",
        path_with_trailing_slash: "foo/",
        path_with_fancy_components: "./foo/../../../foo",
    }

    macro_rules! file_tests {
        ($($name:ident: $filename:literal,)*) => {
        $(
            #[test]
            fn $name() {
                let mock_typeshed = mock_typeshed();

                let path = FileSystemPath::new($filename);

                assert!(mock_typeshed.exists(path));
                assert!(mock_typeshed.is_file(path));
                assert!(!mock_typeshed.is_directory(path));

                let path_metadata = mock_typeshed.metadata(path).unwrap();
                assert!(path_metadata.file_type().is_file());
                assert!(path_metadata.permissions().is_some());
                assert_eq!(path_metadata.revision(), FileRevision::zero());
            }
        )*
        }
    }

    file_tests! {
        functools: "stdlib/functools.pyi",
        asyncio_tasks: "stdlib/asyncio/tasks.pyi",
        functools_parent_components: "stdlib/../stdlib/../stdlib/functools.pyi",
        asyncio_tasks_odd_components: "./stdlib/asyncio/../asyncio/tasks.pyi",
    }

    #[test]
    fn functools_file_contents() {
        let mock_typeshed = mock_typeshed();
        let functools = mock_typeshed
            .read(FileSystemPath::new("stdlib/functools.pyi"))
            .unwrap();
        assert_eq!(functools.as_str(), FUNCTOOLS_CONTENTS);
    }

    #[test]
    fn asyncio_tasks_file_contents() {
        let mock_typeshed = mock_typeshed();
        let functools = mock_typeshed
            .read(FileSystemPath::new("stdlib/asyncio/tasks.pyi"))
            .unwrap();
        assert_eq!(functools.as_str(), ASYNCIO_TASKS_CONTENTS);
    }
}
