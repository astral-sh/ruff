use crate::system::{SystemPath, SystemPathBuf};
use crate::vendored::{VendoredPath, VendoredPathBuf};

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
    System(SystemPathBuf),
    Vendored(VendoredPathBuf),
}

impl VfsPath {
    /// Create a new path to a file on the file system.
    #[must_use]
    pub fn system(path: impl AsRef<SystemPath>) -> Self {
        VfsPath::System(path.as_ref().to_path_buf())
    }

    /// Returns `Some` if the path is a file system path that points to a path on disk.
    #[must_use]
    #[inline]
    pub fn into_system_path_buf(self) -> Option<SystemPathBuf> {
        match self {
            VfsPath::System(path) => Some(path),
            VfsPath::Vendored(_) => None,
        }
    }

    #[must_use]
    #[inline]
    pub fn as_system_path(&self) -> Option<&SystemPath> {
        match self {
            VfsPath::System(path) => Some(path.as_path()),
            VfsPath::Vendored(_) => None,
        }
    }

    /// Returns `true` if the path is a file system path that points to a path on disk.
    #[must_use]
    #[inline]
    pub const fn is_system_path(&self) -> bool {
        matches!(self, VfsPath::System(_))
    }

    /// Returns `true` if the path is a vendored path.
    #[must_use]
    #[inline]
    pub const fn is_vendored_path(&self) -> bool {
        matches!(self, VfsPath::Vendored(_))
    }

    #[must_use]
    #[inline]
    pub fn as_vendored_path(&self) -> Option<&VendoredPath> {
        match self {
            VfsPath::Vendored(path) => Some(path.as_path()),
            VfsPath::System(_) => None,
        }
    }

    /// Yields the underlying [`str`] slice.
    pub fn as_str(&self) -> &str {
        match self {
            VfsPath::System(path) => path.as_str(),
            VfsPath::Vendored(path) => path.as_str(),
        }
    }
}

impl AsRef<str> for VfsPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<SystemPathBuf> for VfsPath {
    fn from(value: SystemPathBuf) -> Self {
        Self::System(value)
    }
}

impl From<&SystemPath> for VfsPath {
    fn from(value: &SystemPath) -> Self {
        VfsPath::System(value.to_path_buf())
    }
}

impl From<VendoredPathBuf> for VfsPath {
    fn from(value: VendoredPathBuf) -> Self {
        Self::Vendored(value)
    }
}

impl From<&VendoredPath> for VfsPath {
    fn from(value: &VendoredPath) -> Self {
        Self::Vendored(value.to_path_buf())
    }
}

impl PartialEq<SystemPath> for VfsPath {
    #[inline]
    fn eq(&self, other: &SystemPath) -> bool {
        self.as_system_path()
            .is_some_and(|self_path| self_path == other)
    }
}

impl PartialEq<VfsPath> for SystemPath {
    #[inline]
    fn eq(&self, other: &VfsPath) -> bool {
        other == self
    }
}

impl PartialEq<SystemPathBuf> for VfsPath {
    #[inline]
    fn eq(&self, other: &SystemPathBuf) -> bool {
        self == other.as_path()
    }
}

impl PartialEq<VfsPath> for SystemPathBuf {
    fn eq(&self, other: &VfsPath) -> bool {
        other == self
    }
}

impl PartialEq<VendoredPath> for VfsPath {
    #[inline]
    fn eq(&self, other: &VendoredPath) -> bool {
        self.as_vendored_path()
            .is_some_and(|self_path| self_path == other)
    }
}

impl PartialEq<VfsPath> for VendoredPath {
    #[inline]
    fn eq(&self, other: &VfsPath) -> bool {
        other == self
    }
}

impl PartialEq<VendoredPathBuf> for VfsPath {
    #[inline]
    fn eq(&self, other: &VendoredPathBuf) -> bool {
        other.as_path() == self
    }
}

impl PartialEq<VfsPath> for VendoredPathBuf {
    #[inline]
    fn eq(&self, other: &VfsPath) -> bool {
        other == self
    }
}
