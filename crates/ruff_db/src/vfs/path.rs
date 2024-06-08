use camino::Utf8PathBuf;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum VfsPath {
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

    /// Returns the final component of the [`VfsPath`], if there is one.
    ///
    /// If the path is a normal file, this is the file name. If it's the path of a directory, this
    /// is the directory name.
    ///
    /// Returns [`None`] if the path terminates in `..`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::vfs::VfsPath;
    ///
    /// assert_eq!(Some("bin"), VfsPath::fs_from_str("/usr/bin/").file_name());
    /// assert_eq!(Some("foo.txt"), VfsPath::fs_from_str("tmp/foo.txt").file_name());
    /// assert_eq!(Some("foo.txt"), VfsPath::fs_from_str("foo.txt/.").file_name());
    /// assert_eq!(Some("foo.txt"), VfsPath::fs_from_str("foo.txt/.//").file_name());
    /// assert_eq!(None, VfsPath::fs_from_str("foo.txt/..").file_name());
    /// assert_eq!(None, VfsPath::fs_from_str("/").file_name());
    /// ```
    pub fn file_name(&self) -> Option<&str> {
        match self {
            VfsPath::Fs(path) => path.file_name(),
            VfsPath::Vendored(path) => path.file_name(),
        }
    }

    /// Extracts the extension of [`self.file_name`], if possible.
    ///
    /// The extension is:
    ///
    /// * [`None`], if there is no file name;
    /// * [`None`], if there is no embedded `.`;
    /// * [`None`], if the file name begins with `.` and has no other `.`s within;
    /// * Otherwise, the portion of the file name after the final `.`
    ///
    /// [`self.file_name`]: VfsPath::file_name
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::vfs::VfsPath;
    ///
    /// assert_eq!("rs", VfsPath::fs_from_str("foo.rs").extension().unwrap());
    /// assert_eq!("gz", VfsPath::fs_from_str("foo.tar.gz").extension().unwrap());
    /// ```
    #[must_use]
    #[inline]
    pub fn extension(&self) -> Option<&str> {
        match self {
            VfsPath::Fs(path) => path.extension(),
            VfsPath::Vendored(path) => path.extension(),
        }
    }

    /// Extracts the stem (non-extension) portion of [`self.file_name`].
    ///
    /// [`self.file_name`]: VfsPath::file_name
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
    /// use ruff_db::vfs::VfsPath;
    ///
    /// assert_eq!("foo", VfsPath::fs_from_str("foo.rs").file_stem().unwrap());
    /// assert_eq!("foo.tar", VfsPath::fs_from_str("foo.tar.gz").file_stem().unwrap());
    /// ```
    #[must_use]
    #[inline]
    pub fn file_stem(&self) -> Option<&str> {
        match self {
            VfsPath::Fs(path) => path.file_stem(),
            VfsPath::Vendored(path) => path.file_stem(),
        }
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
