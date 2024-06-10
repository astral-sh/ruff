use std::fmt::Formatter;
use std::ops::Deref;
use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use filetime::FileTime;

pub use memory::MemoryFileSystem;
pub use os::OsFileSystem;

mod memory;
mod os;

pub type Result<T> = std::io::Result<T>;

/// A file system that can be used to read and write files.
///
/// The file system is agnostic to the actual storage medium, it could be a real file system, a combination
/// of a real file system and an in-memory file system in the case of an LSP where unsaved changes are stored in memory,
/// or an all in-memory file system for testing.
pub trait FileSystem {
    /// Reads the metadata of the file or directory at `path`.
    fn metadata(&self, path: &FileSystemPath) -> Result<Metadata>;

    /// Reads the content of the file at `path`.
    fn read(&self, path: &FileSystemPath) -> Result<String>;

    /// Returns `true` if `path` exists.
    fn exists(&self, path: &FileSystemPath) -> bool;
}

// TODO support untitled files for the LSP use case. Wrap a `str` and `String`
//    The main question is how `as_std_path` would work for untitled files, that can only exist in the LSP case
//    but there's no compile time guarantee that a [`OsFileSystem`] never gets an untitled file path.

/// Path to a file or directory stored in [`FileSystem`].
///
/// The path is guaranteed to be valid UTF-8.
#[repr(transparent)]
#[derive(Eq, PartialEq, Hash)]
pub struct FileSystemPath(Utf8Path);

impl FileSystemPath {
    pub fn new(path: &(impl AsRef<Utf8Path> + ?Sized)) -> &Self {
        let path = path.as_ref();
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

    /// Converts the path to an owned [`FileSystemPathBuf`].
    pub fn to_path_buf(&self) -> FileSystemPathBuf {
        FileSystemPathBuf(self.0.to_path_buf())
    }

    /// Returns the path as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the std path for the file.
    pub fn as_std_path(&self) -> &Path {
        self.0.as_std_path()
    }
}

/// Owned path to a file or directory stored in [`FileSystem`].
///
/// The path is guaranteed to be valid UTF-8.
#[repr(transparent)]
#[derive(Eq, PartialEq, Clone, Hash)]
pub struct FileSystemPathBuf(Utf8PathBuf);

impl Default for FileSystemPathBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystemPathBuf {
    pub fn new() -> Self {
        Self(Utf8PathBuf::new())
    }

    pub fn as_path(&self) -> &FileSystemPath {
        // SAFETY: FsPath is marked as #[repr(transparent)] so the conversion from a
        // *const Utf8Path to a *const FsPath is valid.
        unsafe { &*(self.0.as_path() as *const Utf8Path as *const FileSystemPath) }
    }
}

impl AsRef<FileSystemPath> for FileSystemPathBuf {
    fn as_ref(&self) -> &FileSystemPath {
        self.as_path()
    }
}

impl Deref for FileSystemPathBuf {
    type Target = FileSystemPath;

    fn deref(&self) -> &Self::Target {
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

/// A number representing the revision of a file.
///
/// Two revisions that don't compare equal signify that the file has been modified.
/// Revisions aren't guaranteed to be monotonically increasing or in any specific order.
///
/// Possible revisions are:
/// * The last modification time of the file.
/// * The hash of the file's content.
/// * The revision as it comes from an external system, for example the LSP.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FileRevision(u128);

impl FileRevision {
    pub fn new(value: u128) -> Self {
        Self(value)
    }

    pub const fn zero() -> Self {
        Self(0)
    }

    #[must_use]
    pub fn as_u128(self) -> u128 {
        self.0
    }
}

impl From<u128> for FileRevision {
    fn from(value: u128) -> Self {
        FileRevision(value)
    }
}

impl From<u64> for FileRevision {
    fn from(value: u64) -> Self {
        FileRevision(u128::from(value))
    }
}

impl From<FileTime> for FileRevision {
    fn from(value: FileTime) -> Self {
        let seconds = value.seconds() as u128;
        let seconds = seconds << 64;
        let nanos = value.nanoseconds() as u128;

        FileRevision(seconds | nanos)
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

#[cfg(test)]
mod tests {
    use filetime::FileTime;

    use crate::file_system::FileRevision;

    #[test]
    fn revision_from_file_time() {
        let file_time = FileTime::now();
        let revision = FileRevision::from(file_time);

        let revision = revision.as_u128();

        let nano = revision & 0xFFFF_FFFF_FFFF_FFFF;
        let seconds = revision >> 64;

        assert_eq!(file_time.nanoseconds(), nano as u32);
        assert_eq!(file_time.seconds(), seconds as i64);
    }
}
