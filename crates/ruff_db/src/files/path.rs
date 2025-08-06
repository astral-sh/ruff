use crate::Db;
use crate::files::{File, system_path_to_file, vendored_path_to_file};
use crate::system::{SystemPath, SystemPathBuf, SystemVirtualPath, SystemVirtualPathBuf};
use crate::vendored::{VendoredPath, VendoredPathBuf};
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
        Self::System(path.as_ref().to_path_buf())
    }

    /// Returns `Some` if the path is a file system path that points to a path on disk.
    #[must_use]
    #[inline]
    pub fn into_system_path_buf(self) -> Option<SystemPathBuf> {
        match self {
            Self::System(path) => Some(path),
            Self::Vendored(_) | Self::SystemVirtual(_) => None,
        }
    }

    #[must_use]
    #[inline]
    pub fn as_system_path(&self) -> Option<&SystemPath> {
        match self {
            Self::System(path) => Some(path.as_path()),
            Self::Vendored(_) | Self::SystemVirtual(_) => None,
        }
    }

    /// Returns `true` if the path is a file system path that points to a path on disk.
    #[must_use]
    #[inline]
    pub const fn is_system_path(&self) -> bool {
        matches!(self, Self::System(_))
    }

    /// Returns `true` if the path is a file system path that is virtual i.e., it doesn't exists on
    /// disk.
    #[must_use]
    #[inline]
    pub const fn is_system_virtual_path(&self) -> bool {
        matches!(self, Self::SystemVirtual(_))
    }

    /// Returns `true` if the path is a vendored path.
    #[must_use]
    #[inline]
    pub const fn is_vendored_path(&self) -> bool {
        matches!(self, Self::Vendored(_))
    }

    #[must_use]
    #[inline]
    pub fn as_vendored_path(&self) -> Option<&VendoredPath> {
        match self {
            Self::Vendored(path) => Some(path.as_path()),
            Self::System(_) | Self::SystemVirtual(_) => None,
        }
    }

    /// Yields the underlying [`str`] slice.
    pub fn as_str(&self) -> &str {
        match self {
            Self::System(path) => path.as_str(),
            Self::Vendored(path) => path.as_str(),
            Self::SystemVirtual(path) => path.as_str(),
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
            Self::System(path) => system_path_to_file(db, path).ok(),
            Self::Vendored(path) => vendored_path_to_file(db, path).ok(),
            Self::SystemVirtual(_) => None,
        }
    }

    #[must_use]
    pub fn extension(&self) -> Option<&str> {
        match self {
            Self::System(path) => path.extension(),
            Self::Vendored(path) => path.extension(),
            Self::SystemVirtual(_) => None,
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
        Self::System(value.to_path_buf())
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
        Self::SystemVirtual(value.to_path_buf())
    }
}

impl From<SystemVirtualPathBuf> for FilePath {
    fn from(value: SystemVirtualPathBuf) -> Self {
        Self::SystemVirtual(value)
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
            Self::System(path) => std::fmt::Display::fmt(path, f),
            Self::SystemVirtual(path) => std::fmt::Display::fmt(path, f),
            Self::Vendored(path) => std::fmt::Display::fmt(path, f),
        }
    }
}
