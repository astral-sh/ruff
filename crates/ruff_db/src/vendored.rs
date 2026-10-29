use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{self, Debug};
use std::io;
#[cfg(feature = "deflate")]
use std::io::Read;
use std::ops::Range;
use std::sync::Arc;

pub use self::path::{VendoredPath, VendoredPathBuf};
use crate::file_revision::FileRevision;

mod path;

type Result<T> = io::Result<T>;

/// File system that stores all content in a sorted index and data blob
/// that can be bundled as part of the binary.
///
/// "Files" in the `VendoredFileSystem` are read-only and immutable.
/// Directories are supported, but symlinks and hardlinks cannot exist.
///
/// # Path separators
///
/// Paths in the index use `/` separators and have no trailing slash. On Windows, callers may
/// also use `\` separators, which are normalized to `/` before lookup.
#[derive(Clone, Default)]
pub struct VendoredFileSystem {
    inner: Arc<VendoredData>,
}

impl VendoredFileSystem {
    /// Construct a file system from a build-generated index and content blob.
    ///
    /// Entries must have distinct, normalized paths in lexicographic order. Each file's byte
    /// range identifies one independently compressed payload in `data`.
    pub fn new_static(
        entries: &'static [VendoredEntry],
        data: &'static [u8],
        compression: Compression,
    ) -> Result<Self> {
        if !entries.windows(2).all(|pair| pair[0].path < pair[1].path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "vendored paths must be sorted and unique",
            ));
        }

        Ok(Self {
            inner: Arc::new(VendoredData {
                entries: Cow::Borrowed(entries),
                data: Cow::Borrowed(data),
                compression,
            }),
        })
    }

    fn entry(&self, path: &VendoredPath) -> Option<&VendoredEntry> {
        let normalized = NormalizedVendoredPath::from(path);
        self.inner
            .entries
            .binary_search_by(|entry| entry.path.as_ref().cmp(normalized.as_str()))
            .ok()
            .map(|index| &self.inner.entries[index])
    }

    pub fn exists(&self, path: impl AsRef<VendoredPath>) -> bool {
        self.entry(path.as_ref()).is_some()
    }

    pub fn metadata(&self, path: impl AsRef<VendoredPath>) -> Result<Metadata> {
        let entry = self
            .entry(path.as_ref())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file not found"))?;
        Ok(Metadata {
            kind: entry.kind,
            revision: FileRevision::new(u128::from(entry.crc32_hash)),
        })
    }

    pub fn is_directory(&self, path: impl AsRef<VendoredPath>) -> bool {
        self.entry(path.as_ref())
            .is_some_and(|entry| entry.kind.is_directory())
    }

    pub fn is_file(&self, path: impl AsRef<VendoredPath>) -> bool {
        self.entry(path.as_ref())
            .is_some_and(|entry| entry.kind.is_file())
    }

    /// Read the entire contents of the vendored file at `path` into a string
    ///
    /// Returns an error if any of the following are true:
    /// - The path does not exist in the underlying index
    /// - The path exists in the underlying index, but represents a directory
    /// - The indexed byte range or compressed contents are invalid
    /// - The contents do not match the indexed size or CRC32 hash
    /// - The contents of the file at `path` contain invalid UTF-8
    pub fn read_to_string(&self, path: impl AsRef<VendoredPath>) -> Result<String> {
        let entry = self
            .entry(path.as_ref())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file not found"))?;
        if entry.kind.is_directory() {
            return Err(io::Error::new(
                io::ErrorKind::IsADirectory,
                "vendored path is a directory",
            ));
        }
        let data = self.inner.data.get(entry.range.clone()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "invalid vendored file range")
        })?;
        let contents = match self.inner.compression {
            Compression::Stored => data.to_vec(),
            #[cfg(feature = "zstd")]
            Compression::Zstd => zstd::bulk::decompress(data, entry.uncompressed_size)?,
            #[cfg(feature = "deflate")]
            Compression::Deflate => {
                // Limit the initial allocation because the indexed size may be invalid.
                let mut contents = Vec::with_capacity(entry.uncompressed_size.min(10_000_000));
                flate2::read::DeflateDecoder::new(data)
                    .take(entry.uncompressed_size as u64 + 1)
                    .read_to_end(&mut contents)?;
                contents
            }
        };
        if contents.len() != entry.uncompressed_size
            || crc32fast::hash(&contents) != entry.crc32_hash
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "vendored file contents do not match the index",
            ));
        }
        String::from_utf8(contents)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    /// Read the direct children of the directory
    /// identified by `dir`.
    ///
    /// Directory entry paths end in `/`.
    pub fn read_directory(
        &self,
        dir: impl AsRef<VendoredPath>,
    ) -> impl Iterator<Item = DirectoryEntry> + '_ {
        let prefix = NormalizedVendoredPath::from(dir.as_ref())
            .with_trailing_slash()
            .0
            .into_owned();
        let start = self
            .inner
            .entries
            .partition_point(|entry| entry.path.as_ref() < prefix.as_str());

        self.inner.entries[start..]
            .iter()
            .map_while(move |entry| {
                let relative = entry.path.strip_prefix(&prefix)?;
                Some((entry, !relative.is_empty() && !relative.contains('/')))
            })
            .filter(|(_, is_direct_child)| *is_direct_child)
            .map(|(entry, _)| DirectoryEntry {
                path: if entry.kind.is_directory() {
                    VendoredPathBuf::from(format!("{}/", entry.path).as_str())
                } else {
                    VendoredPathBuf::from(entry.path.as_ref())
                },
                file_type: entry.kind,
            })
    }
}

impl fmt::Debug for VendoredFileSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            let paths: Vec<&str> = self
                .inner
                .entries
                .iter()
                .map(|entry| entry.path.as_ref())
                .collect();
            let debug_info: BTreeMap<&str, FileDebugInfo> = self
                .inner
                .entries
                .iter()
                .map(|entry| {
                    (
                        entry.path.as_ref(),
                        FileDebugInfo {
                            crc32_hash: entry.crc32_hash,
                            compressed_size: entry.range.len(),
                            uncompressed_size: entry.uncompressed_size,
                            kind: entry.kind,
                        },
                    )
                })
                .collect();
            f.debug_struct("VendoredFileSystem")
                .field("paths", &paths)
                .field("data_by_path", &debug_info)
                .finish()
        } else {
            write!(
                f,
                "VendoredFileSystem(<{} paths>)",
                self.inner.entries.len()
            )
        }
    }
}

/// Independently compressed file contents use the same codec throughout a file system.
#[derive(Clone, Copy, Debug, Default)]
pub enum Compression {
    #[default]
    Stored,
    #[cfg(feature = "zstd")]
    Zstd,
    #[cfg(feature = "deflate")]
    Deflate,
}

#[derive(Default)]
struct VendoredData {
    entries: Cow<'static, [VendoredEntry]>,
    data: Cow<'static, [u8]>,
    compression: Compression,
}

/// An index entry emitted by the vendored typeshed build script.
#[derive(Clone, Debug)]
pub struct VendoredEntry {
    path: Cow<'static, str>,
    range: Range<usize>,
    uncompressed_size: usize,
    crc32_hash: u32,
    kind: FileType,
}

impl VendoredEntry {
    pub const fn file(
        path: &'static str,
        range: Range<usize>,
        uncompressed_size: usize,
        crc32_hash: u32,
    ) -> Self {
        Self {
            path: Cow::Borrowed(path),
            range,
            uncompressed_size,
            crc32_hash,
            kind: FileType::File,
        }
    }

    pub const fn directory(path: &'static str) -> Self {
        Self {
            path: Cow::Borrowed(path),
            range: 0..0,
            uncompressed_size: 0,
            crc32_hash: 0,
            kind: FileType::Directory,
        }
    }
}

/// Private struct only used in `Debug` implementations
///
/// This could possibly be unified with the `Metadata` struct,
/// but that is deliberately kept small, and only exposes metadata
/// that users of the `VendoredFileSystem` could realistically need.
/// For debugging purposes, however, we want to have all information
/// available.
#[expect(unused)]
#[derive(Debug)]
struct FileDebugInfo {
    crc32_hash: u32,
    compressed_size: usize,
    uncompressed_size: usize,
    kind: FileType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileType {
    /// The path exists in the index and represents a vendored file
    File,

    /// The path exists in the index and represents a vendored directory of files
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
    pub fn kind(&self) -> FileType {
        self.kind
    }

    pub fn revision(&self) -> FileRevision {
        self.revision
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DirectoryEntry {
    path: VendoredPathBuf,
    file_type: FileType,
}

impl DirectoryEntry {
    pub fn new(path: VendoredPathBuf, file_type: FileType) -> Self {
        Self { path, file_type }
    }

    pub fn into_path(self) -> VendoredPathBuf {
        self.path
    }

    pub fn path(&self) -> &VendoredPath {
        &self.path
    }

    pub fn file_type(&self) -> FileType {
        self.file_type
    }
}

/// A path that has been normalized via the `normalize_vendored_path` function.
///
#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedVendoredPath<'a>(Cow<'a, str>);

impl NormalizedVendoredPath<'_> {
    fn with_trailing_slash(self) -> Self {
        debug_assert!(!self.0.ends_with('/'));
        let mut data = self.0.into_owned();
        data.push('/');
        Self(Cow::Owned(data))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'a> From<&'a VendoredPath> for NormalizedVendoredPath<'a> {
    /// Normalize the path.
    ///
    /// The normalizations are:
    /// - Remove `.` and `..` components
    /// - Strip trailing slashes
    /// - Normalize `\\` separators to `/`
    /// - Validate that the path does not have any unsupported components
    ///
    /// ## Panics:
    /// If a path with an unsupported component for vendored paths is passed.
    /// Unsupported components are path prefixes and path root directories.
    fn from(path: &'a VendoredPath) -> Self {
        /// Remove `.` and `..` components, and validate that unsupported components are not present.
        ///
        /// This inner routine also strips trailing slashes,
        /// and normalizes paths to use Unix `/` separators.
        /// However, it always allocates, so avoid calling it if possible.
        /// In most cases, the path should already be normalized.
        fn normalize_unnormalized_path(path: &VendoredPath) -> String {
            let mut normalized_parts = Vec::new();
            for component in path.components() {
                match component {
                    camino::Utf8Component::Normal(part) => normalized_parts.push(part),
                    camino::Utf8Component::CurDir => continue,
                    camino::Utf8Component::ParentDir => {
                        // `VendoredPath("")`, `VendoredPath("..")` and `VendoredPath("../..")`
                        // all resolve to the same path relative to the vendored file system
                        // (see https://github.com/astral-sh/ruff/pull/11991#issuecomment-2185278014)
                        normalized_parts.pop();
                    }
                    unsupported => {
                        panic!("Unsupported component in a vendored path: {unsupported}")
                    }
                }
            }
            normalized_parts.join("/")
        }

        let path_str = path.as_str();

        if std::path::MAIN_SEPARATOR == '\\' && path_str.contains('\\') {
            // Normalize paths so that they always use Unix path separators
            NormalizedVendoredPath(Cow::Owned(normalize_unnormalized_path(path)))
        } else if !path
            .components()
            .all(|component| matches!(component, camino::Utf8Component::Normal(_)))
        {
            // Remove non-`Normal` components
            NormalizedVendoredPath(Cow::Owned(normalize_unnormalized_path(path)))
        } else {
            // Strip trailing slashes from the path
            NormalizedVendoredPath(Cow::Borrowed(path_str.trim_end_matches('/')))
        }
    }
}

/// Build an owned, uncompressed file system for synthetic vendored files.
#[derive(Default)]
pub struct VendoredFileSystemBuilder {
    entries: BTreeMap<String, VendoredEntry>,
    data: Vec<u8>,
}

impl VendoredFileSystemBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&mut self, path: impl AsRef<VendoredPath>, content: &str) -> Result<()> {
        let path = NormalizedVendoredPath::from(path.as_ref()).0.into_owned();
        self.check_new_path(&path)?;
        let start = self.data.len();
        self.data.extend_from_slice(content.as_bytes());
        self.entries.insert(
            path.clone(),
            VendoredEntry {
                path: Cow::Owned(path),
                range: start..self.data.len(),
                uncompressed_size: content.len(),
                crc32_hash: crc32fast::hash(content.as_bytes()),
                kind: FileType::File,
            },
        );
        Ok(())
    }

    pub fn add_directory(&mut self, path: impl AsRef<VendoredPath>) -> Result<()> {
        let path = NormalizedVendoredPath::from(path.as_ref()).0.into_owned();
        self.check_new_path(&path)?;
        self.entries.insert(
            path.clone(),
            VendoredEntry {
                path: Cow::Owned(path),
                range: 0..0,
                uncompressed_size: 0,
                crc32_hash: 0,
                kind: FileType::Directory,
            },
        );
        Ok(())
    }

    fn check_new_path(&self, path: &str) -> Result<()> {
        if self.entries.contains_key(path) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "duplicate vendored path",
            ));
        }
        Ok(())
    }

    pub fn finish(mut self) -> VendoredFileSystem {
        let mut entries: Vec<_> = self.entries.into_values().collect();
        entries.shrink_to_fit();
        self.data.shrink_to_fit();
        VendoredFileSystem {
            inner: Arc::new(VendoredData {
                entries: Cow::Owned(entries),
                data: Cow::Owned(self.data),
                compression: Compression::Stored,
            }),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {

    use insta::assert_snapshot;

    use super::*;

    const FUNCTOOLS_CONTENTS: &str = "def update_wrapper(): ...";
    const ASYNCIO_TASKS_CONTENTS: &str = "class Task: ...";

    fn mock_typeshed() -> VendoredFileSystem {
        let mut builder = VendoredFileSystemBuilder::new();

        builder.add_directory("stdlib/").unwrap();
        builder
            .add_file("stdlib/functools.pyi", FUNCTOOLS_CONTENTS)
            .unwrap();
        builder.add_directory("stdlib/asyncio/").unwrap();
        builder
            .add_file("stdlib/asyncio/tasks.pyi", ASYNCIO_TASKS_CONTENTS)
            .unwrap();

        builder.finish()
    }

    #[test]
    fn filesystem_debug_implementation() {
        assert_snapshot!(
            format!("{:?}", mock_typeshed()),
            @"VendoredFileSystem(<4 paths>)"
        );
    }

    #[test]
    fn filesystem_debug_implementation_alternate() {
        assert_snapshot!(format!("{:#?}", mock_typeshed()), @r#"
        VendoredFileSystem {
            paths: [
                "stdlib",
                "stdlib/asyncio",
                "stdlib/asyncio/tasks.pyi",
                "stdlib/functools.pyi",
            ],
            data_by_path: {
                "stdlib": FileDebugInfo {
                    crc32_hash: 0,
                    compressed_size: 0,
                    uncompressed_size: 0,
                    kind: Directory,
                },
                "stdlib/asyncio": FileDebugInfo {
                    crc32_hash: 0,
                    compressed_size: 0,
                    uncompressed_size: 0,
                    kind: Directory,
                },
                "stdlib/asyncio/tasks.pyi": FileDebugInfo {
                    crc32_hash: 2826547428,
                    compressed_size: 15,
                    uncompressed_size: 15,
                    kind: File,
                },
                "stdlib/functools.pyi": FileDebugInfo {
                    crc32_hash: 1099005079,
                    compressed_size: 25,
                    uncompressed_size: 25,
                    kind: File,
                },
            },
        }
        "#);
    }

    fn test_directory(dirname: &str) {
        let mock_typeshed = mock_typeshed();

        let path = VendoredPath::new(dirname);

        assert!(mock_typeshed.exists(path));
        assert!(mock_typeshed.read_to_string(path).is_err());
        let metadata = mock_typeshed.metadata(path).unwrap();
        assert!(metadata.kind().is_directory());
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

    fn readdir_snapshot(fs: &VendoredFileSystem, path: &str) -> String {
        let mut paths = fs
            .read_directory(VendoredPath::new(path))
            .map(|entry| entry.path().to_string())
            .collect::<Vec<String>>();
        paths.sort();
        paths.join("\n")
    }

    #[test]
    fn read_directory_stdlib() {
        let mock_typeshed = mock_typeshed();

        assert_snapshot!(readdir_snapshot(&mock_typeshed, "stdlib"), @"
        vendored://stdlib/asyncio/
        vendored://stdlib/functools.pyi
        ");
        assert_snapshot!(readdir_snapshot(&mock_typeshed, "stdlib/"), @"
        vendored://stdlib/asyncio/
        vendored://stdlib/functools.pyi
        ");
        assert_snapshot!(readdir_snapshot(&mock_typeshed, "./stdlib"), @"
        vendored://stdlib/asyncio/
        vendored://stdlib/functools.pyi
        ");
        assert_snapshot!(readdir_snapshot(&mock_typeshed, "./stdlib/"), @"
        vendored://stdlib/asyncio/
        vendored://stdlib/functools.pyi
        ");
    }

    #[test]
    fn read_directory_asyncio() {
        let mock_typeshed = mock_typeshed();

        assert_snapshot!(
            readdir_snapshot(&mock_typeshed, "stdlib/asyncio"),
            @"vendored://stdlib/asyncio/tasks.pyi",
        );
        assert_snapshot!(
            readdir_snapshot(&mock_typeshed, "./stdlib/asyncio"),
            @"vendored://stdlib/asyncio/tasks.pyi",
        );
        assert_snapshot!(
            readdir_snapshot(&mock_typeshed, "stdlib/asyncio/"),
            @"vendored://stdlib/asyncio/tasks.pyi",
        );
        assert_snapshot!(
            readdir_snapshot(&mock_typeshed, "./stdlib/asyncio/"),
            @"vendored://stdlib/asyncio/tasks.pyi",
        );
    }

    fn test_nonexistent_path(path: &str) {
        let mock_typeshed = mock_typeshed();
        let path = VendoredPath::new(path);
        assert!(!mock_typeshed.exists(path));
        assert!(mock_typeshed.metadata(path).is_err());
        assert!(
            mock_typeshed
                .read_to_string(path)
                .is_err_and(|err| err.to_string().contains("file not found"))
        );
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
        assert!(metadata.kind().is_file());
    }

    #[test]
    fn functools_file_contents() {
        let mock_typeshed = mock_typeshed();
        let path = VendoredPath::new("stdlib/functools.pyi");
        test_file(&mock_typeshed, path);
        let functools_stub = mock_typeshed.read_to_string(path).unwrap();
        assert_eq!(functools_stub.as_str(), FUNCTOOLS_CONTENTS);
        let functools_stub_again = mock_typeshed.read_to_string(path).unwrap();
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
        let asyncio_stub = mock_typeshed.read_to_string(path).unwrap();
        assert_eq!(asyncio_stub.as_str(), ASYNCIO_TASKS_CONTENTS);
    }

    #[test]
    fn asyncio_file_other_path() {
        test_file(
            &mock_typeshed(),
            VendoredPath::new("./stdlib/asyncio/../asyncio/tasks.pyi"),
        )
    }

    #[test]
    fn static_index_requires_sorted_unique_paths() {
        static UNSORTED: &[VendoredEntry] =
            &[VendoredEntry::directory("b"), VendoredEntry::directory("a")];
        static DUPLICATE: &[VendoredEntry] =
            &[VendoredEntry::directory("a"), VendoredEntry::directory("a")];
        for entries in [UNSORTED, DUPLICATE] {
            assert_eq!(
                VendoredFileSystem::new_static(entries, b"", Compression::Stored)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData,
            );
        }
    }

    #[test]
    fn static_file_integrity() {
        static ENTRIES: &[VendoredEntry] = &[
            VendoredEntry::file("bad_checksum", 0..5, 5, 0),
            VendoredEntry::file("bad_range", 0..6, 5, 0x3610_a686),
            VendoredEntry::file("bad_size", 0..5, 4, 0x3610_a686),
            VendoredEntry::file("empty", 0..0, 0, 0),
            VendoredEntry::file("valid", 0..5, 5, 0x3610_a686),
        ];
        let fs = VendoredFileSystem::new_static(ENTRIES, b"hello", Compression::Stored).unwrap();
        assert_eq!(fs.read_to_string("valid").unwrap(), "hello");
        assert_eq!(fs.read_to_string("empty").unwrap(), "");
        for path in ["bad_checksum", "bad_range", "bad_size"] {
            assert_eq!(
                fs.read_to_string(path).unwrap_err().kind(),
                io::ErrorKind::InvalidData,
            );
        }
    }

    #[test]
    fn directory_listing_excludes_descendants_and_sibling_prefixes() {
        let mut builder = VendoredFileSystemBuilder::new();
        builder.add_file("package/sub/child.pyi", "").unwrap();
        builder.add_file("package_suffix/other.pyi", "").unwrap();
        builder.add_file("package/module.pyi", "").unwrap();
        builder.add_directory("package/sub").unwrap();
        builder.add_directory("package").unwrap();
        let fs = builder.finish();
        assert_eq!(
            readdir_snapshot(&fs, "package"),
            "vendored://package/module.pyi\nvendored://package/sub/",
        );
        assert_eq!(readdir_snapshot(&fs, "missing"), "");
        assert_eq!(readdir_snapshot(&fs, "package/module.pyi"), "");
    }

    #[test]
    fn builder_rejects_duplicate_normalized_paths() {
        let mut builder = VendoredFileSystemBuilder::new();
        builder.add_file("stdlib/module.pyi", "original").unwrap();
        assert_eq!(
            builder
                .add_file("./stdlib/module.pyi", "replacement")
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists,
        );
        assert_eq!(
            builder
                .finish()
                .read_to_string("stdlib/module.pyi")
                .unwrap(),
            "original"
        );
    }
}
