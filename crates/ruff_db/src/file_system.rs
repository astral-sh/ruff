use std::fmt::Formatter;
use std::ops::Deref;
use std::path::{Path, StripPrefixError};

use camino::{Utf8Path, Utf8PathBuf};

use crate::file_revision::FileRevision;
pub use memory::MemoryFileSystem;
pub use os::OsFileSystem;

mod memory;
mod os;

pub type Result<T> = std::io::Result<T>;

/// An abstraction over `std::fs` with features tailored to Ruff's needs.
///
/// Provides a file system agnostic API to interact with files and directories.
/// Abstracting the file system operations enables:
///
/// * Accessing unsaved or even untitled files in the LSP use case
/// * Testing with an in-memory file system
/// * Running Ruff in a WASM environment without needing to stub out the full `std::fs` API.
pub trait FileSystem {
    /// Reads the metadata of the file or directory at `path`.
    fn metadata(&self, path: &FileSystemPath) -> Result<Metadata>;

    /// Reads the content of the file at `path`.
    fn read(&self, path: &FileSystemPath) -> Result<String>;

    /// Returns `true` if `path` exists.
    fn exists(&self, path: &FileSystemPath) -> bool;

    /// Returns `true` if `path` exists and is a directory.
    fn is_directory(&self, path: &FileSystemPath) -> bool {
        self.metadata(path)
            .map_or(false, |metadata| metadata.file_type.is_directory())
    }

    /// Returns `true` if `path` exists and is a file.
    fn is_file(&self, path: &FileSystemPath) -> bool {
        self.metadata(path)
            .map_or(false, |metadata| metadata.file_type.is_file())
    }
}

// TODO support untitled files for the LSP use case. Wrap a `str` and `String`
//    The main question is how `as_std_path` would work for untitled files, that can only exist in the LSP case
//    but there's no compile time guarantee that a [`OsFileSystem`] never gets an untitled file path.

/// Path to a file or directory stored in [`FileSystem`].
///
/// The path is guaranteed to be valid UTF-8.
#[repr(transparent)]
#[derive(Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct FileSystemPath(Utf8Path);

impl FileSystemPath {
    pub fn new(path: &(impl AsRef<Utf8Path> + ?Sized)) -> &Self {
        let path = path.as_ref();
        // SAFETY: FsPath is marked as #[repr(transparent)] so the conversion from a
        // *const Utf8Path to a *const FsPath is valid.
        unsafe { &*(path as *const Utf8Path as *const FileSystemPath) }
    }

    /// Extracts the file extension, if possible.
    ///
    /// The extension is:
    ///
    /// * [`None`], if there is no file name;
    /// * [`None`], if there is no embedded `.`;
    /// * [`None`], if the file name begins with `.` and has no other `.`s within;
    /// * Otherwise, the portion of the file name after the final `.`
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::file_system::FileSystemPath;
    ///
    /// assert_eq!("rs", FileSystemPath::new("foo.rs").extension().unwrap());
    /// assert_eq!("gz", FileSystemPath::new("foo.tar.gz").extension().unwrap());
    /// ```
    ///
    /// See [`Path::extension`] for more details.
    #[inline]
    #[must_use]
    pub fn extension(&self) -> Option<&str> {
        self.0.extension()
    }

    /// Determines whether `base` is a prefix of `self`.
    ///
    /// Only considers whole path components to match.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::file_system::FileSystemPath;
    ///
    /// let path = FileSystemPath::new("/etc/passwd");
    ///
    /// assert!(path.starts_with("/etc"));
    /// assert!(path.starts_with("/etc/"));
    /// assert!(path.starts_with("/etc/passwd"));
    /// assert!(path.starts_with("/etc/passwd/")); // extra slash is okay
    /// assert!(path.starts_with("/etc/passwd///")); // multiple extra slashes are okay
    ///
    /// assert!(!path.starts_with("/e"));
    /// assert!(!path.starts_with("/etc/passwd.txt"));
    ///
    /// assert!(!FileSystemPath::new("/etc/foo.rs").starts_with("/etc/foo"));
    /// ```
    #[inline]
    #[must_use]
    pub fn starts_with(&self, base: impl AsRef<FileSystemPath>) -> bool {
        self.0.starts_with(base.as_ref())
    }

    /// Determines whether `child` is a suffix of `self`.
    ///
    /// Only considers whole path components to match.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::file_system::FileSystemPath;
    ///
    /// let path = FileSystemPath::new("/etc/resolv.conf");
    ///
    /// assert!(path.ends_with("resolv.conf"));
    /// assert!(path.ends_with("etc/resolv.conf"));
    /// assert!(path.ends_with("/etc/resolv.conf"));
    ///
    /// assert!(!path.ends_with("/resolv.conf"));
    /// assert!(!path.ends_with("conf")); // use .extension() instead
    /// ```
    #[inline]
    #[must_use]
    pub fn ends_with(&self, child: impl AsRef<FileSystemPath>) -> bool {
        self.0.ends_with(child.as_ref())
    }

    /// Returns the `FileSystemPath` without its final component, if there is one.
    ///
    /// Returns [`None`] if the path terminates in a root or prefix.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::file_system::FileSystemPath;
    ///
    /// let path = FileSystemPath::new("/foo/bar");
    /// let parent = path.parent().unwrap();
    /// assert_eq!(parent, FileSystemPath::new("/foo"));
    ///
    /// let grand_parent = parent.parent().unwrap();
    /// assert_eq!(grand_parent, FileSystemPath::new("/"));
    /// assert_eq!(grand_parent.parent(), None);
    /// ```
    #[inline]
    #[must_use]
    pub fn parent(&self) -> Option<&FileSystemPath> {
        self.0.parent().map(FileSystemPath::new)
    }

    /// Produces an iterator over the [`camino::Utf8Component`]s of the path.
    ///
    /// When parsing the path, there is a small amount of normalization:
    ///
    /// * Repeated separators are ignored, so `a/b` and `a//b` both have
    ///   `a` and `b` as components.
    ///
    /// * Occurrences of `.` are normalized away, except if they are at the
    ///   beginning of the path. For example, `a/./b`, `a/b/`, `a/b/.` and
    ///   `a/b` all have `a` and `b` as components, but `./a/b` starts with
    ///   an additional [`CurDir`] component.
    ///
    /// * A trailing slash is normalized away, `/a/b` and `/a/b/` are equivalent.
    ///
    /// Note that no other normalization takes place; in particular, `a/c`
    /// and `a/b/../c` are distinct, to account for the possibility that `b`
    /// is a symbolic link (so its parent isn't `a`).
    ///
    /// # Examples
    ///
    /// ```
    /// use camino::{Utf8Component};
    /// use ruff_db::file_system::FileSystemPath;
    ///
    /// let mut components = FileSystemPath::new("/tmp/foo.txt").components();
    ///
    /// assert_eq!(components.next(), Some(Utf8Component::RootDir));
    /// assert_eq!(components.next(), Some(Utf8Component::Normal("tmp")));
    /// assert_eq!(components.next(), Some(Utf8Component::Normal("foo.txt")));
    /// assert_eq!(components.next(), None)
    /// ```
    ///
    /// [`CurDir`]: camino::Utf8Component::CurDir
    #[inline]
    pub fn components(&self) -> camino::Utf8Components {
        self.0.components()
    }

    /// Returns the final component of the `FileSystemPath`, if there is one.
    ///
    /// If the path is a normal file, this is the file name. If it's the path of a directory, this
    /// is the directory name.
    ///
    /// Returns [`None`] if the path terminates in `..`.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino::Utf8Path;
    /// use ruff_db::file_system::FileSystemPath;
    ///
    /// assert_eq!(Some("bin"), FileSystemPath::new("/usr/bin/").file_name());
    /// assert_eq!(Some("foo.txt"), FileSystemPath::new("tmp/foo.txt").file_name());
    /// assert_eq!(Some("foo.txt"), FileSystemPath::new("foo.txt/.").file_name());
    /// assert_eq!(Some("foo.txt"), FileSystemPath::new("foo.txt/.//").file_name());
    /// assert_eq!(None, FileSystemPath::new("foo.txt/..").file_name());
    /// assert_eq!(None, FileSystemPath::new("/").file_name());
    /// ```
    #[inline]
    #[must_use]
    pub fn file_name(&self) -> Option<&str> {
        self.0.file_name()
    }

    /// Extracts the stem (non-extension) portion of [`self.file_name`].
    ///
    /// [`self.file_name`]: FileSystemPath::file_name
    ///
    /// The stem is:
    ///
    /// * [`None`], if there is no file name;
    /// * The entire file name if there is no embedded `.`;
    /// * The entire file name if the file name begins with `.` and has no other `.`s within;
    /// * Otherwise, the portion of the file name before the final `.`
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::file_system::FileSystemPath;
    ///
    /// assert_eq!("foo", FileSystemPath::new("foo.rs").file_stem().unwrap());
    /// assert_eq!("foo.tar", FileSystemPath::new("foo.tar.gz").file_stem().unwrap());
    /// ```
    #[inline]
    #[must_use]
    pub fn file_stem(&self) -> Option<&str> {
        self.0.file_stem()
    }

    /// Returns a path that, when joined onto `base`, yields `self`.
    ///
    /// # Errors
    ///
    /// If `base` is not a prefix of `self` (i.e., [`starts_with`]
    /// returns `false`), returns [`Err`].
    ///
    /// [`starts_with`]: FileSystemPath::starts_with
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::file_system::{FileSystemPath, FileSystemPathBuf};
    ///
    /// let path = FileSystemPath::new("/test/haha/foo.txt");
    ///
    /// assert_eq!(path.strip_prefix("/"), Ok(FileSystemPath::new("test/haha/foo.txt")));
    /// assert_eq!(path.strip_prefix("/test"), Ok(FileSystemPath::new("haha/foo.txt")));
    /// assert_eq!(path.strip_prefix("/test/"), Ok(FileSystemPath::new("haha/foo.txt")));
    /// assert_eq!(path.strip_prefix("/test/haha/foo.txt"), Ok(FileSystemPath::new("")));
    /// assert_eq!(path.strip_prefix("/test/haha/foo.txt/"), Ok(FileSystemPath::new("")));
    ///
    /// assert!(path.strip_prefix("test").is_err());
    /// assert!(path.strip_prefix("/haha").is_err());
    ///
    /// let prefix = FileSystemPathBuf::from("/test/");
    /// assert_eq!(path.strip_prefix(prefix), Ok(FileSystemPath::new("haha/foo.txt")));
    /// ```
    #[inline]
    pub fn strip_prefix(
        &self,
        base: impl AsRef<FileSystemPath>,
    ) -> std::result::Result<&FileSystemPath, StripPrefixError> {
        self.0.strip_prefix(base.as_ref()).map(FileSystemPath::new)
    }

    /// Creates an owned [`FileSystemPathBuf`] with `path` adjoined to `self`.
    ///
    /// See [`std::path::PathBuf::push`] for more details on what it means to adjoin a path.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::file_system::{FileSystemPath, FileSystemPathBuf};
    ///
    /// assert_eq!(FileSystemPath::new("/etc").join("passwd"), FileSystemPathBuf::from("/etc/passwd"));
    /// ```
    #[inline]
    #[must_use]
    pub fn join(&self, path: impl AsRef<FileSystemPath>) -> FileSystemPathBuf {
        FileSystemPathBuf::from_utf8_path_buf(self.0.join(&path.as_ref().0))
    }

    /// Creates an owned [`FileSystemPathBuf`] like `self` but with the given extension.
    ///
    /// See [`std::path::PathBuf::set_extension`] for more details.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::file_system::{FileSystemPath, FileSystemPathBuf};
    ///
    /// let path = FileSystemPath::new("foo.rs");
    /// assert_eq!(path.with_extension("txt"), FileSystemPathBuf::from("foo.txt"));
    ///
    /// let path = FileSystemPath::new("foo.tar.gz");
    /// assert_eq!(path.with_extension(""), FileSystemPathBuf::from("foo.tar"));
    /// assert_eq!(path.with_extension("xz"), FileSystemPathBuf::from("foo.tar.xz"));
    /// assert_eq!(path.with_extension("").with_extension("txt"), FileSystemPathBuf::from("foo.txt"));
    /// ```
    #[inline]
    pub fn with_extension(&self, extension: &str) -> FileSystemPathBuf {
        FileSystemPathBuf::from_utf8_path_buf(self.0.with_extension(extension))
    }

    /// Converts the path to an owned [`FileSystemPathBuf`].
    pub fn to_path_buf(&self) -> FileSystemPathBuf {
        FileSystemPathBuf(self.0.to_path_buf())
    }

    /// Returns the path as a string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the std path for the file.
    #[inline]
    pub fn as_std_path(&self) -> &Path {
        self.0.as_std_path()
    }

    pub fn from_std_path(path: &Path) -> Option<&FileSystemPath> {
        Some(FileSystemPath::new(Utf8Path::from_path(path)?))
    }
}

/// Owned path to a file or directory stored in [`FileSystem`].
///
/// The path is guaranteed to be valid UTF-8.
#[repr(transparent)]
#[derive(Eq, PartialEq, Clone, Hash, PartialOrd, Ord)]
pub struct FileSystemPathBuf(Utf8PathBuf);

impl FileSystemPathBuf {
    pub fn new() -> Self {
        Self(Utf8PathBuf::new())
    }

    pub fn from_utf8_path_buf(path: Utf8PathBuf) -> Self {
        Self(path)
    }

    pub fn from_path_buf(
        path: std::path::PathBuf,
    ) -> std::result::Result<Self, std::path::PathBuf> {
        Utf8PathBuf::from_path_buf(path).map(Self)
    }

    /// Extends `self` with `path`.
    ///
    /// If `path` is absolute, it replaces the current path.
    ///
    /// On Windows:
    ///
    /// * if `path` has a root but no prefix (e.g., `\windows`), it
    ///   replaces everything except for the prefix (if any) of `self`.
    /// * if `path` has a prefix but no root, it replaces `self`.
    ///
    /// # Examples
    ///
    /// Pushing a relative path extends the existing path:
    ///
    /// ```
    /// use ruff_db::file_system::FileSystemPathBuf;
    ///
    /// let mut path = FileSystemPathBuf::from("/tmp");
    /// path.push("file.bk");
    /// assert_eq!(path, FileSystemPathBuf::from("/tmp/file.bk"));
    /// ```
    ///
    /// Pushing an absolute path replaces the existing path:
    ///
    /// ```
    ///
    /// use ruff_db::file_system::FileSystemPathBuf;
    ///
    /// let mut path = FileSystemPathBuf::from("/tmp");
    /// path.push("/etc");
    /// assert_eq!(path, FileSystemPathBuf::from("/etc"));
    /// ```
    pub fn push(&mut self, path: impl AsRef<FileSystemPath>) {
        self.0.push(&path.as_ref().0);
    }

    #[inline]
    pub fn as_path(&self) -> &FileSystemPath {
        FileSystemPath::new(&self.0)
    }
}

impl From<&str> for FileSystemPathBuf {
    fn from(value: &str) -> Self {
        FileSystemPathBuf::from_utf8_path_buf(Utf8PathBuf::from(value))
    }
}

impl Default for FileSystemPathBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl AsRef<FileSystemPath> for FileSystemPathBuf {
    #[inline]
    fn as_ref(&self) -> &FileSystemPath {
        self.as_path()
    }
}

impl AsRef<FileSystemPath> for FileSystemPath {
    #[inline]
    fn as_ref(&self) -> &FileSystemPath {
        self
    }
}

impl AsRef<FileSystemPath> for str {
    #[inline]
    fn as_ref(&self) -> &FileSystemPath {
        FileSystemPath::new(self)
    }
}

impl AsRef<FileSystemPath> for String {
    #[inline]
    fn as_ref(&self) -> &FileSystemPath {
        FileSystemPath::new(self)
    }
}

impl AsRef<Path> for FileSystemPath {
    #[inline]
    fn as_ref(&self) -> &Path {
        self.0.as_std_path()
    }
}

impl Deref for FileSystemPathBuf {
    type Target = FileSystemPath;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_path()
    }
}

impl std::fmt::Debug for FileSystemPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for FileSystemPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Debug for FileSystemPathBuf {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for FileSystemPathBuf {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metadata {
    revision: FileRevision,
    permissions: Option<u32>,
    file_type: FileType,
}

impl Metadata {
    pub fn revision(&self) -> FileRevision {
        self.revision
    }

    pub fn permissions(&self) -> Option<u32> {
        self.permissions
    }

    pub fn file_type(&self) -> FileType {
        self.file_type
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum FileType {
    File,
    Directory,
    Symlink,
}

impl FileType {
    pub const fn is_file(self) -> bool {
        matches!(self, FileType::File)
    }

    pub const fn is_directory(self) -> bool {
        matches!(self, FileType::Directory)
    }

    pub const fn is_symlink(self) -> bool {
        matches!(self, FileType::Symlink)
    }
}
