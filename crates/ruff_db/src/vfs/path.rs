use std::ops::Deref;
use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};

use crate::file_system::{FileSystemPath, FileSystemPathBuf};

#[repr(transparent)]
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct VendoredPath(Utf8Path);

impl VendoredPath {
    pub fn new(path: &(impl AsRef<Utf8Path> + ?Sized)) -> &Self {
        let path = path.as_ref();
        unsafe { &*(path as *const Utf8Path as *const VendoredPath) }
    }

    pub fn to_path_buf(&self) -> VendoredPathBuf {
        VendoredPathBuf(self.0.to_path_buf())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[repr(transparent)]
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct VendoredPathBuf(Utf8PathBuf);

impl Default for VendoredPathBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl VendoredPathBuf {
    pub fn new() -> Self {
        Self(Utf8PathBuf::new())
    }

    pub fn as_path(&self) -> &VendoredPath {
        // SAFETY: VendoredPath is marked as #[repr(transparent)] so the conversion from a
        // *const Utf8Path to a *const VendoredPath is valid.
        unsafe { &*(self.0.as_path() as *const Utf8Path as *const VendoredPath) }
    }
}

impl AsRef<VendoredPath> for VendoredPathBuf {
    fn as_ref(&self) -> &VendoredPath {
        self.as_path()
    }
}

impl AsRef<VendoredPath> for VendoredPath {
    #[inline]
    fn as_ref(&self) -> &VendoredPath {
        self
    }
}

impl AsRef<VendoredPath> for str {
    #[inline]
    fn as_ref(&self) -> &VendoredPath {
        VendoredPath::new(self)
    }
}

impl AsRef<VendoredPath> for String {
    #[inline]
    fn as_ref(&self) -> &VendoredPath {
        VendoredPath::new(self)
    }
}

impl AsRef<Path> for VendoredPath {
    #[inline]
    fn as_ref(&self) -> &Path {
        self.0.as_std_path()
    }
}

impl Deref for VendoredPathBuf {
    type Target = VendoredPath;

    fn deref(&self) -> &Self::Target {
        self.as_path()
    }
}

/// Path to a file.
///
/// The path abstracts that files in Ruff can come from different sources:
///
/// * a file stored on disk
/// * a vendored file that ships as part of the ruff binary
/// * Future: A virtual file that references a slice of another file. For example, the CSS code in a python file.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum VfsPath {
    /// Path that points to a file on disk.
    FileSystem(FileSystemPathBuf),
    Vendored(VendoredPathBuf),
}

impl VfsPath {
    /// Create a new path to a file on the file system.
    #[must_use]
    pub fn file_system(path: impl AsRef<FileSystemPath>) -> Self {
        VfsPath::FileSystem(path.as_ref().to_path_buf())
    }

    /// Returns `Some` if the path is a file system path that points to a path on disk.
    #[must_use]
    pub fn into_file_system_path_buf(self) -> Option<FileSystemPathBuf> {
        match self {
            VfsPath::FileSystem(path) => Some(path),
            VfsPath::Vendored(_) => None,
        }
    }

    /// Returns `true` if the path is a file system path that points to a path on disk.
    #[must_use]
    pub const fn is_file_system_path(&self) -> bool {
        matches!(self, VfsPath::FileSystem(_))
    }

    /// Yields the underlying [`str`] slice.
    pub fn as_str(&self) -> &str {
        match self {
            VfsPath::FileSystem(path) => path.as_str(),
            VfsPath::Vendored(path) => path.as_str(),
        }
    }
}

impl AsRef<str> for VfsPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
