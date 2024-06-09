use camino::Utf8PathBuf;

/// Path to a file.
///
/// The path abstracts that files in Ruff can come from different sources:
///
/// * a file stored on disk
/// * a vendored file that ships as part of the ruff binary
/// * Future: A virtual file that references a slice of another file. For example, the CSS code in a python file.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum VfsPath {
    // TODO: How to represent untitled files in the editor? Use our own `Path` type that's just a thin wrapper around
    //       str/String instead?
    /// Path that points to
    ///
    ///
    /// a file or directory on disk.
    Fs(Utf8PathBuf),
    Vendored(Utf8PathBuf),
}

impl VfsPath {
    /// Create a new path to a file on the file system.
    #[must_use]
    pub fn fs(path: Utf8PathBuf) -> Self {
        VfsPath::Fs(path)
    }

    /// Creates a new FS path from a string.
    pub fn fs_from_str(path: &str) -> Self {
        VfsPath::Fs(path.into())
    }

    /// Returns `Some` if the path is a file system path that points to a path on disk.
    #[must_use]
    pub fn into_fs_path_buf(self) -> Option<Utf8PathBuf> {
        match self {
            VfsPath::Fs(path) => Some(path),
            VfsPath::Vendored(_) => None,
        }
    }

    /// Returns `true` if the path is a file system path that points to a path on disk.
    #[must_use]
    pub const fn is_fs_path(&self) -> bool {
        matches!(self, VfsPath::Fs(_))
    }

    /// Yields the underlying [`str`] slice.
    pub fn as_str(&self) -> &str {
        match self {
            VfsPath::Fs(path) => path.as_str(),
            VfsPath::Vendored(path) => path.as_str(),
        }
    }
}

impl AsRef<str> for VfsPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
