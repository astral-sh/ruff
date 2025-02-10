use camino::{Utf8Path, Utf8PathBuf};
use std::borrow::Borrow;
use std::fmt::Formatter;
use std::ops::Deref;
use std::path::{Path, PathBuf, StripPrefixError};

/// A slice of a path on [`System`](super::System) (akin to [`str`]).
///
/// The path is guaranteed to be valid UTF-8.
#[repr(transparent)]
#[derive(Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct SystemPath(Utf8Path);

impl SystemPath {
    pub fn new(path: &(impl AsRef<Utf8Path> + ?Sized)) -> &Self {
        let path = path.as_ref();
        // SAFETY: FsPath is marked as #[repr(transparent)] so the conversion from a
        // *const Utf8Path to a *const FsPath is valid.
        unsafe { &*(path as *const Utf8Path as *const SystemPath) }
    }

    /// Takes any path, and when possible, converts Windows UNC paths to regular paths.
    /// If the path can't be converted, it's returned unmodified.
    ///
    /// On non-Windows this is no-op.
    ///
    /// `\\?\C:\Windows` will be converted to `C:\Windows`,
    /// but `\\?\C:\COM` will be left as-is (due to a reserved filename).
    ///
    /// Use this to pass arbitrary paths to programs that may not be UNC-aware.
    ///
    /// It's generally safe to pass UNC paths to legacy programs, because
    /// these paths contain a reserved prefix, so will gracefully fail
    /// if used with legacy APIs that don't support UNC.
    ///
    /// This function does not perform any I/O.
    ///
    /// Currently paths with unpaired surrogates aren't converted even if they
    /// could be, due to limitations of Rust's `OsStr` API.
    ///
    /// To check if a path remained as UNC, use `path.as_os_str().as_encoded_bytes().starts_with(b"\\\\")`.
    #[inline]
    pub fn simplified(&self) -> &SystemPath {
        // SAFETY: simplified only trims the path, that means the returned path must be a valid UTF-8 path.
        SystemPath::from_std_path(dunce::simplified(self.as_std_path())).unwrap()
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
    /// use ruff_db::system::SystemPath;
    ///
    /// assert_eq!("rs", SystemPath::new("foo.rs").extension().unwrap());
    /// assert_eq!("gz", SystemPath::new("foo.tar.gz").extension().unwrap());
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
    /// use ruff_db::system::SystemPath;
    ///
    /// let path = SystemPath::new("/etc/passwd");
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
    /// assert!(!SystemPath::new("/etc/foo.rs").starts_with("/etc/foo"));
    /// ```
    #[inline]
    #[must_use]
    pub fn starts_with(&self, base: impl AsRef<SystemPath>) -> bool {
        self.0.starts_with(base.as_ref())
    }

    /// Determines whether `child` is a suffix of `self`.
    ///
    /// Only considers whole path components to match.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::system::SystemPath;
    ///
    /// let path = SystemPath::new("/etc/resolv.conf");
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
    pub fn ends_with(&self, child: impl AsRef<SystemPath>) -> bool {
        self.0.ends_with(child.as_ref())
    }

    /// Returns the `FileSystemPath` without its final component, if there is one.
    ///
    /// Returns [`None`] if the path terminates in a root or prefix.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::system::SystemPath;
    ///
    /// let path = SystemPath::new("/foo/bar");
    /// let parent = path.parent().unwrap();
    /// assert_eq!(parent, SystemPath::new("/foo"));
    ///
    /// let grand_parent = parent.parent().unwrap();
    /// assert_eq!(grand_parent, SystemPath::new("/"));
    /// assert_eq!(grand_parent.parent(), None);
    /// ```
    #[inline]
    #[must_use]
    pub fn parent(&self) -> Option<&SystemPath> {
        self.0.parent().map(SystemPath::new)
    }

    /// Produces an iterator over `SystemPath` and its ancestors.
    ///
    /// The iterator will yield the `SystemPath` that is returned if the [`parent`] method is used zero
    /// or more times. That means, the iterator will yield `&self`, `&self.parent().unwrap()`,
    /// `&self.parent().unwrap().parent().unwrap()` and so on. If the [`parent`] method returns
    /// [`None`], the iterator will do likewise. The iterator will always yield at least one value,
    /// namely `&self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::system::SystemPath;
    ///
    /// let mut ancestors = SystemPath::new("/foo/bar").ancestors();
    /// assert_eq!(ancestors.next(), Some(SystemPath::new("/foo/bar")));
    /// assert_eq!(ancestors.next(), Some(SystemPath::new("/foo")));
    /// assert_eq!(ancestors.next(), Some(SystemPath::new("/")));
    /// assert_eq!(ancestors.next(), None);
    ///
    /// let mut ancestors = SystemPath::new("../foo/bar").ancestors();
    /// assert_eq!(ancestors.next(), Some(SystemPath::new("../foo/bar")));
    /// assert_eq!(ancestors.next(), Some(SystemPath::new("../foo")));
    /// assert_eq!(ancestors.next(), Some(SystemPath::new("..")));
    /// assert_eq!(ancestors.next(), Some(SystemPath::new("")));
    /// assert_eq!(ancestors.next(), None);
    /// ```
    ///
    /// [`parent`]: SystemPath::parent
    #[inline]
    pub fn ancestors(&self) -> impl Iterator<Item = &SystemPath> {
        self.0.ancestors().map(SystemPath::new)
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
    /// use ruff_db::system::SystemPath;
    ///
    /// let mut components = SystemPath::new("/tmp/foo.txt").components();
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
    /// use ruff_db::system::SystemPath;
    ///
    /// assert_eq!(Some("bin"), SystemPath::new("/usr/bin/").file_name());
    /// assert_eq!(Some("foo.txt"), SystemPath::new("tmp/foo.txt").file_name());
    /// assert_eq!(Some("foo.txt"), SystemPath::new("foo.txt/.").file_name());
    /// assert_eq!(Some("foo.txt"), SystemPath::new("foo.txt/.//").file_name());
    /// assert_eq!(None, SystemPath::new("foo.txt/..").file_name());
    /// assert_eq!(None, SystemPath::new("/").file_name());
    /// ```
    #[inline]
    #[must_use]
    pub fn file_name(&self) -> Option<&str> {
        self.0.file_name()
    }

    /// Extracts the stem (non-extension) portion of [`self.file_name`].
    ///
    /// [`self.file_name`]: SystemPath::file_name
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
    /// use ruff_db::system::SystemPath;
    ///
    /// assert_eq!("foo", SystemPath::new("foo.rs").file_stem().unwrap());
    /// assert_eq!("foo.tar", SystemPath::new("foo.tar.gz").file_stem().unwrap());
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
    /// [`starts_with`]: SystemPath::starts_with
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::system::{SystemPath, SystemPathBuf};
    ///
    /// let path = SystemPath::new("/test/haha/foo.txt");
    ///
    /// assert_eq!(path.strip_prefix("/"), Ok(SystemPath::new("test/haha/foo.txt")));
    /// assert_eq!(path.strip_prefix("/test"), Ok(SystemPath::new("haha/foo.txt")));
    /// assert_eq!(path.strip_prefix("/test/"), Ok(SystemPath::new("haha/foo.txt")));
    /// assert_eq!(path.strip_prefix("/test/haha/foo.txt"), Ok(SystemPath::new("")));
    /// assert_eq!(path.strip_prefix("/test/haha/foo.txt/"), Ok(SystemPath::new("")));
    ///
    /// assert!(path.strip_prefix("test").is_err());
    /// assert!(path.strip_prefix("/haha").is_err());
    ///
    /// let prefix = SystemPathBuf::from("/test/");
    /// assert_eq!(path.strip_prefix(prefix), Ok(SystemPath::new("haha/foo.txt")));
    /// ```
    #[inline]
    pub fn strip_prefix(
        &self,
        base: impl AsRef<SystemPath>,
    ) -> std::result::Result<&SystemPath, StripPrefixError> {
        self.0.strip_prefix(base.as_ref()).map(SystemPath::new)
    }

    /// Creates an owned [`SystemPathBuf`] with `path` adjoined to `self`.
    ///
    /// See [`std::path::PathBuf::push`] for more details on what it means to adjoin a path.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::system::{SystemPath, SystemPathBuf};
    ///
    /// assert_eq!(SystemPath::new("/etc").join("passwd"), SystemPathBuf::from("/etc/passwd"));
    /// ```
    #[inline]
    #[must_use]
    pub fn join(&self, path: impl AsRef<SystemPath>) -> SystemPathBuf {
        SystemPathBuf::from_utf8_path_buf(self.0.join(&path.as_ref().0))
    }

    /// Creates an owned [`SystemPathBuf`] like `self` but with the given extension.
    ///
    /// See [`std::path::PathBuf::set_extension`] for more details.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::system::{SystemPath, SystemPathBuf};
    ///
    /// let path = SystemPath::new("foo.rs");
    /// assert_eq!(path.with_extension("txt"), SystemPathBuf::from("foo.txt"));
    ///
    /// let path = SystemPath::new("foo.tar.gz");
    /// assert_eq!(path.with_extension(""), SystemPathBuf::from("foo.tar"));
    /// assert_eq!(path.with_extension("xz"), SystemPathBuf::from("foo.tar.xz"));
    /// assert_eq!(path.with_extension("").with_extension("txt"), SystemPathBuf::from("foo.txt"));
    /// ```
    #[inline]
    pub fn with_extension(&self, extension: &str) -> SystemPathBuf {
        SystemPathBuf::from_utf8_path_buf(self.0.with_extension(extension))
    }

    /// Converts the path to an owned [`SystemPathBuf`].
    pub fn to_path_buf(&self) -> SystemPathBuf {
        SystemPathBuf(self.0.to_path_buf())
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

    /// Returns the [`Utf8Path`] for the file.
    #[inline]
    pub fn as_utf8_path(&self) -> &Utf8Path {
        &self.0
    }

    pub fn from_std_path(path: &Path) -> Option<&SystemPath> {
        Some(SystemPath::new(Utf8Path::from_path(path)?))
    }

    /// Makes a path absolute and normalizes it without accessing the file system.
    ///
    /// Adapted from [cargo](https://github.com/rust-lang/cargo/blob/fede83ccf973457de319ba6fa0e36ead454d2e20/src/cargo/util/paths.rs#L61)
    ///
    /// # Examples
    ///
    /// ## Posix paths
    ///
    /// ```
    /// # #[cfg(unix)]
    /// # fn main() {
    ///   use ruff_db::system::{SystemPath, SystemPathBuf};
    ///
    ///   // Relative to absolute
    ///   let absolute = SystemPath::absolute("foo/./bar", "/tmp");
    ///   assert_eq!(absolute, SystemPathBuf::from("/tmp/foo/bar"));
    ///
    ///   // Path's going past the root are normalized to the root
    ///   let absolute = SystemPath::absolute("../../../", "/tmp");
    ///   assert_eq!(absolute, SystemPathBuf::from("/"));
    ///
    ///   // Absolute to absolute
    ///   let absolute = SystemPath::absolute("/foo//test/.././bar.rs", "/tmp");
    ///   assert_eq!(absolute, SystemPathBuf::from("/foo/bar.rs"));
    /// # }
    /// # #[cfg(not(unix))]
    /// # fn main() {}
    /// ```
    ///
    /// ## Windows paths
    ///
    /// ```
    /// # #[cfg(windows)]
    /// # fn main() {
    ///   use ruff_db::system::{SystemPath, SystemPathBuf};
    ///
    ///   // Relative to absolute
    ///   let absolute = SystemPath::absolute(r"foo\.\bar", r"C:\tmp");
    ///   assert_eq!(absolute, SystemPathBuf::from(r"C:\tmp\foo\bar"));
    ///
    ///   // Path's going past the root are normalized to the root
    ///   let absolute = SystemPath::absolute(r"..\..\..\", r"C:\tmp");
    ///   assert_eq!(absolute, SystemPathBuf::from(r"C:\"));
    ///
    ///   // Absolute to absolute
    ///   let absolute = SystemPath::absolute(r"C:\foo//test\..\./bar.rs", r"C:\tmp");
    ///   assert_eq!(absolute, SystemPathBuf::from(r"C:\foo\bar.rs"));
    /// # }
    /// # #[cfg(not(windows))]
    /// # fn main() {}
    /// ```
    pub fn absolute(path: impl AsRef<SystemPath>, cwd: impl AsRef<SystemPath>) -> SystemPathBuf {
        fn absolute(path: &SystemPath, cwd: &SystemPath) -> SystemPathBuf {
            let path = &path.0;

            let mut components = path.components().peekable();
            let mut ret = if let Some(
                c @ (camino::Utf8Component::Prefix(..) | camino::Utf8Component::RootDir),
            ) = components.peek().cloned()
            {
                components.next();
                Utf8PathBuf::from(c.as_str())
            } else {
                cwd.0.to_path_buf()
            };

            for component in components {
                match component {
                    camino::Utf8Component::Prefix(..) => unreachable!(),
                    camino::Utf8Component::RootDir => {
                        ret.push(component);
                    }
                    camino::Utf8Component::CurDir => {}
                    camino::Utf8Component::ParentDir => {
                        ret.pop();
                    }
                    camino::Utf8Component::Normal(c) => {
                        ret.push(c);
                    }
                }
            }

            SystemPathBuf::from_utf8_path_buf(ret)
        }

        absolute(path.as_ref(), cwd.as_ref())
    }
}

impl ToOwned for SystemPath {
    type Owned = SystemPathBuf;

    fn to_owned(&self) -> Self::Owned {
        self.to_path_buf()
    }
}

/// An owned, mutable path on [`System`](`super::System`) (akin to [`String`]).
///
/// The path is guaranteed to be valid UTF-8.
#[repr(transparent)]
#[derive(Eq, PartialEq, Clone, Hash, PartialOrd, Ord)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct SystemPathBuf(#[cfg_attr(feature = "schemars", schemars(with = "String"))] Utf8PathBuf);

impl SystemPathBuf {
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
    /// use ruff_db::system::SystemPathBuf;
    ///
    /// let mut path = SystemPathBuf::from("/tmp");
    /// path.push("file.bk");
    /// assert_eq!(path, SystemPathBuf::from("/tmp/file.bk"));
    /// ```
    ///
    /// Pushing an absolute path replaces the existing path:
    ///
    /// ```
    ///
    /// use ruff_db::system::SystemPathBuf;
    ///
    /// let mut path = SystemPathBuf::from("/tmp");
    /// path.push("/etc");
    /// assert_eq!(path, SystemPathBuf::from("/etc"));
    /// ```
    pub fn push(&mut self, path: impl AsRef<SystemPath>) {
        self.0.push(&path.as_ref().0);
    }

    pub fn into_utf8_path_buf(self) -> Utf8PathBuf {
        self.0
    }

    pub fn into_std_path_buf(self) -> PathBuf {
        self.0.into_std_path_buf()
    }

    #[inline]
    pub fn as_path(&self) -> &SystemPath {
        SystemPath::new(&self.0)
    }
}

impl Borrow<SystemPath> for SystemPathBuf {
    fn borrow(&self) -> &SystemPath {
        self.as_path()
    }
}

impl From<&str> for SystemPathBuf {
    fn from(value: &str) -> Self {
        SystemPathBuf::from_utf8_path_buf(Utf8PathBuf::from(value))
    }
}

impl From<String> for SystemPathBuf {
    fn from(value: String) -> Self {
        SystemPathBuf::from_utf8_path_buf(Utf8PathBuf::from(value))
    }
}

impl Default for SystemPathBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl AsRef<SystemPath> for SystemPathBuf {
    #[inline]
    fn as_ref(&self) -> &SystemPath {
        self.as_path()
    }
}

impl AsRef<SystemPath> for SystemPath {
    #[inline]
    fn as_ref(&self) -> &SystemPath {
        self
    }
}

impl AsRef<SystemPath> for Utf8Path {
    #[inline]
    fn as_ref(&self) -> &SystemPath {
        SystemPath::new(self)
    }
}

impl AsRef<SystemPath> for Utf8PathBuf {
    #[inline]
    fn as_ref(&self) -> &SystemPath {
        SystemPath::new(self.as_path())
    }
}

impl AsRef<SystemPath> for str {
    #[inline]
    fn as_ref(&self) -> &SystemPath {
        SystemPath::new(self)
    }
}

impl AsRef<SystemPath> for String {
    #[inline]
    fn as_ref(&self) -> &SystemPath {
        SystemPath::new(self)
    }
}

impl AsRef<Path> for SystemPath {
    #[inline]
    fn as_ref(&self) -> &Path {
        self.0.as_std_path()
    }
}

impl Deref for SystemPathBuf {
    type Target = SystemPath;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_path()
    }
}

impl std::fmt::Debug for SystemPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for SystemPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Debug for SystemPathBuf {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for SystemPathBuf {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(feature = "cache")]
impl ruff_cache::CacheKey for SystemPath {
    fn cache_key(&self, hasher: &mut ruff_cache::CacheKeyHasher) {
        self.0.as_str().cache_key(hasher);
    }
}

#[cfg(feature = "cache")]
impl ruff_cache::CacheKey for SystemPathBuf {
    fn cache_key(&self, hasher: &mut ruff_cache::CacheKeyHasher) {
        self.as_path().cache_key(hasher);
    }
}

/// A slice of a virtual path on [`System`](super::System) (akin to [`str`]).
#[repr(transparent)]
pub struct SystemVirtualPath(str);

impl SystemVirtualPath {
    pub fn new(path: &str) -> &SystemVirtualPath {
        // SAFETY: SystemVirtualPath is marked as #[repr(transparent)] so the conversion from a
        // *const str to a *const SystemVirtualPath is valid.
        unsafe { &*(path as *const str as *const SystemVirtualPath) }
    }

    /// Converts the path to an owned [`SystemVirtualPathBuf`].
    pub fn to_path_buf(&self) -> SystemVirtualPathBuf {
        SystemVirtualPathBuf(self.0.to_string())
    }

    /// Extracts the file extension, if possible.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_db::system::SystemVirtualPath;
    ///
    /// assert_eq!(None, SystemVirtualPath::new("untitled:Untitled-1").extension());
    /// assert_eq!("ipynb", SystemVirtualPath::new("untitled:Untitled-1.ipynb").extension().unwrap());
    /// assert_eq!("ipynb", SystemVirtualPath::new("vscode-notebook-cell:Untitled-1.ipynb").extension().unwrap());
    /// ```
    ///
    /// See [`Path::extension`] for more details.
    pub fn extension(&self) -> Option<&str> {
        Path::new(&self.0).extension().and_then(|ext| ext.to_str())
    }

    /// Returns the path as a string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An owned, virtual path on [`System`](`super::System`) (akin to [`String`]).
#[derive(Eq, PartialEq, Clone, Hash, PartialOrd, Ord)]
pub struct SystemVirtualPathBuf(String);

impl SystemVirtualPathBuf {
    #[inline]
    pub fn as_path(&self) -> &SystemVirtualPath {
        SystemVirtualPath::new(&self.0)
    }
}

impl From<String> for SystemVirtualPathBuf {
    fn from(value: String) -> Self {
        SystemVirtualPathBuf(value)
    }
}

impl AsRef<SystemVirtualPath> for SystemVirtualPathBuf {
    #[inline]
    fn as_ref(&self) -> &SystemVirtualPath {
        self.as_path()
    }
}

impl AsRef<SystemVirtualPath> for SystemVirtualPath {
    #[inline]
    fn as_ref(&self) -> &SystemVirtualPath {
        self
    }
}

impl AsRef<SystemVirtualPath> for str {
    #[inline]
    fn as_ref(&self) -> &SystemVirtualPath {
        SystemVirtualPath::new(self)
    }
}

impl AsRef<SystemVirtualPath> for String {
    #[inline]
    fn as_ref(&self) -> &SystemVirtualPath {
        SystemVirtualPath::new(self)
    }
}

impl Deref for SystemVirtualPathBuf {
    type Target = SystemVirtualPath;

    fn deref(&self) -> &Self::Target {
        self.as_path()
    }
}

impl std::fmt::Debug for SystemVirtualPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for SystemVirtualPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Debug for SystemVirtualPathBuf {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for SystemVirtualPathBuf {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(feature = "cache")]
impl ruff_cache::CacheKey for SystemVirtualPath {
    fn cache_key(&self, hasher: &mut ruff_cache::CacheKeyHasher) {
        self.as_str().cache_key(hasher);
    }
}

#[cfg(feature = "cache")]
impl ruff_cache::CacheKey for SystemVirtualPathBuf {
    fn cache_key(&self, hasher: &mut ruff_cache::CacheKeyHasher) {
        self.as_path().cache_key(hasher);
    }
}

/// Deduplicates identical paths and removes nested paths.
///
/// # Examples
/// ```rust
/// use ruff_db::system::{SystemPath, deduplicate_nested_paths};///
///
/// let paths = vec![SystemPath::new("/a/b/c"), SystemPath::new("/a/b"), SystemPath::new("/a/beta"), SystemPath::new("/a/b/c")];
/// assert_eq!(deduplicate_nested_paths(paths).collect::<Vec<_>>(), &[SystemPath::new("/a/b"), SystemPath::new("/a/beta")]);
/// ```
pub fn deduplicate_nested_paths<P, I>(paths: I) -> DeduplicatedNestedPathsIter<P>
where
    I: IntoIterator<Item = P>,
    P: AsRef<SystemPath>,
{
    DeduplicatedNestedPathsIter::new(paths)
}

pub struct DeduplicatedNestedPathsIter<P> {
    inner: std::vec::IntoIter<P>,
    next: Option<P>,
}

impl<P> DeduplicatedNestedPathsIter<P>
where
    P: AsRef<SystemPath>,
{
    fn new<I>(paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
    {
        let mut paths = paths.into_iter().collect::<Vec<_>>();
        // Sort the path to ensure that e.g. `/a/b/c`, comes right after `/a/b`.
        paths.sort_unstable_by(|left, right| left.as_ref().cmp(right.as_ref()));

        let mut iter = paths.into_iter();

        Self {
            next: iter.next(),
            inner: iter,
        }
    }
}

impl<P> Iterator for DeduplicatedNestedPathsIter<P>
where
    P: AsRef<SystemPath>,
{
    type Item = P;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next.take()?;

        for next in self.inner.by_ref() {
            // Skip all paths that have the same prefix as the current path
            if !next.as_ref().starts_with(current.as_ref()) {
                self.next = Some(next);
                break;
            }
        }

        Some(current)
    }
}
