use crate::files::{system_path_to_file, vendored_path_to_file, File};
use crate::system::{SystemPath, SystemPathBuf, SystemVirtualPath, SystemVirtualPathBuf};
use crate::vendored::{VendoredPath, VendoredPathBuf};
use crate::Db;
use std::fmt::{Display, Formatter};

/// Path to a file.
///
/// The path abstracts that files in Ruff can come from different sources:
///
/// * a file stored on the [host system](crate::system::System).
/// * a virtual file stored on the [host system](crate::system::System).
/// * a vendored file stored in the [vendored file system](crate::vendored::VendoredFileSystem).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum FilePath {
    /// Path to a file on the [host system](crate::system::System).
    System(SystemPathBuf),
    /// Path to a virtual file on the [host system](crate::system::System).
    SystemVirtual(SystemVirtualPathBuf),
    /// Path to a file vendored as part of Ruff. Stored in the [vendored file system](crate::vendored::VendoredFileSystem).
    Vendored(VendoredPathBuf),
}

impl FilePath {
    /// Create a new path to a file on the file system.
    #[must_use]
    pub fn system(path: impl AsRef<SystemPath>) -> Self {
        FilePath::System(path.as_ref().to_path_buf())
    }

    /// Returns `Some` if the path is a file system path that points to a path on disk.
    #[must_use]
    #[inline]
    pub fn into_system_path_buf(self) -> Option<SystemPathBuf> {
        match self {
            FilePath::System(path) => Some(path),
            FilePath::Vendored(_) | FilePath::SystemVirtual(_) => None,
        }
    }

    #[must_use]
    #[inline]
    pub fn as_system_path(&self) -> Option<&SystemPath> {
        match self {
            FilePath::System(path) => Some(path.as_path()),
            FilePath::Vendored(_) | FilePath::SystemVirtual(_) => None,
        }
    }

    /// Returns `true` if the path is a file system path that points to a path on disk.
    #[must_use]
    #[inline]
    pub const fn is_system_path(&self) -> bool {
        matches!(self, FilePath::System(_))
    }

    /// Returns `true` if the path is a file system path that is virtual i.e., it doesn't exists on
    /// disk.
    #[must_use]
    #[inline]
    pub const fn is_system_virtual_path(&self) -> bool {
        matches!(self, FilePath::SystemVirtual(_))
    }

    /// Returns `true` if the path is a vendored path.
    #[must_use]
    #[inline]
    pub const fn is_vendored_path(&self) -> bool {
        matches!(self, FilePath::Vendored(_))
    }

    #[must_use]
    #[inline]
    pub fn as_vendored_path(&self) -> Option<&VendoredPath> {
        match self {
            FilePath::Vendored(path) => Some(path.as_path()),
            FilePath::System(_) | FilePath::SystemVirtual(_) => None,
        }
    }

    /// Yields the underlying [`str`] slice.
    pub fn as_str(&self) -> &str {
        match self {
            FilePath::System(path) => path.as_str(),
            FilePath::Vendored(path) => path.as_str(),
            FilePath::SystemVirtual(path) => path.as_str(),
        }
    }

    /// Interns a virtual file system path and returns a salsa [`File`] ingredient.
    ///
    /// Returns `Some` if a file for `path` exists and is accessible by the user. Returns `None` otherwise.
    ///
    /// See [`system_path_to_file`] or [`vendored_path_to_file`] if you always have either a file
    /// system or vendored path.
    #[inline]
    pub fn to_file(&self, db: &dyn Db) -> Option<File> {
        match self {
            FilePath::System(path) => system_path_to_file(db, path).ok(),
            FilePath::Vendored(path) => vendored_path_to_file(db, path).ok(),
            FilePath::SystemVirtual(_) => None,
        }
    }

    #[must_use]
    pub fn extension(&self) -> Option<&str> {
        match self {
            FilePath::System(path) => path.extension(),
            FilePath::Vendored(path) => path.extension(),
            FilePath::SystemVirtual(_) => None,
        }
    }
}

impl AsRef<str> for FilePath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<SystemPathBuf> for FilePath {
    fn from(value: SystemPathBuf) -> Self {
        Self::System(value)
    }
}

impl From<&SystemPath> for FilePath {
    fn from(value: &SystemPath) -> Self {
        FilePath::System(value.to_path_buf())
    }
}

impl From<VendoredPathBuf> for FilePath {
    fn from(value: VendoredPathBuf) -> Self {
        Self::Vendored(value)
    }
}

impl From<&VendoredPath> for FilePath {
    fn from(value: &VendoredPath) -> Self {
        Self::Vendored(value.to_path_buf())
    }
}

impl From<&SystemVirtualPath> for FilePath {
    fn from(value: &SystemVirtualPath) -> Self {
        FilePath::SystemVirtual(value.to_path_buf())
    }
}

impl From<SystemVirtualPathBuf> for FilePath {
    fn from(value: SystemVirtualPathBuf) -> Self {
        FilePath::SystemVirtual(value)
    }
}

impl PartialEq<SystemPath> for FilePath {
    #[inline]
    fn eq(&self, other: &SystemPath) -> bool {
        self.as_system_path()
            .is_some_and(|self_path| self_path == other)
    }
}

impl PartialEq<FilePath> for SystemPath {
    #[inline]
    fn eq(&self, other: &FilePath) -> bool {
        other == self
    }
}

impl PartialEq<SystemPathBuf> for FilePath {
    #[inline]
    fn eq(&self, other: &SystemPathBuf) -> bool {
        self == other.as_path()
    }
}

impl PartialEq<FilePath> for SystemPathBuf {
    fn eq(&self, other: &FilePath) -> bool {
        other == self
    }
}

impl PartialEq<VendoredPath> for FilePath {
    #[inline]
    fn eq(&self, other: &VendoredPath) -> bool {
        self.as_vendored_path()
            .is_some_and(|self_path| self_path == other)
    }
}

impl PartialEq<FilePath> for VendoredPath {
    #[inline]
    fn eq(&self, other: &FilePath) -> bool {
        other == self
    }
}

impl PartialEq<VendoredPathBuf> for FilePath {
    #[inline]
    fn eq(&self, other: &VendoredPathBuf) -> bool {
        other.as_path() == self
    }
}

impl PartialEq<FilePath> for VendoredPathBuf {
    #[inline]
    fn eq(&self, other: &FilePath) -> bool {
        other == self
    }
}

impl Display for FilePath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FilePath::System(path) => std::fmt::Display::fmt(path, f),
            FilePath::SystemVirtual(path) => std::fmt::Display::fmt(path, f),
            FilePath::Vendored(path) => std::fmt::Display::fmt(path, f),
        }
    }
}
