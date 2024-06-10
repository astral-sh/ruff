use std::sync::{Arc, RwLock, RwLockWriteGuard};

use camino::{Utf8Path, Utf8PathBuf};
use filetime::FileTime;
use rustc_hash::FxHashMap;

use crate::file_system::{FileSystem, FileSystemPath, FileType, Metadata, Result};

/// File system that stores all content in memory.
///
/// The file system supports files and directories. Paths are case-sensitive.
///
/// The implementation doesn't aim at fully capturing the behavior of a real file system.
/// The implementation intentionally doesn't support:
/// * symlinks
/// * hardlinks
/// * permissions: All files and directories have the permission 0755.
///
/// Use a tempdir with the real file system to test these advanced file system features and complex file system behavior.
///
/// Only intended for testing purposes.
pub struct MemoryFileSystem {
    inner: Arc<MemoryFileSystemInner>,
}

impl MemoryFileSystem {
    /// Permission used by all files and directories
    const PERMISSION: u32 = 0o755;

    pub fn new() -> Self {
        Self::with_cwd("/")
    }

    pub fn with_cwd(cwd: impl AsRef<FileSystemPath>) -> Self {
        let cwd = Utf8PathBuf::from(cwd.as_ref().as_str());

        assert!(
            cwd.is_absolute(),
            "The current working directory must be an absolute path."
        );

        let fs = Self {
            inner: Arc::new(MemoryFileSystemInner {
                by_path: RwLock::new(FxHashMap::default()),
                cwd: cwd.clone(),
            }),
        };

        fs.create_directory_all(FileSystemPath::new(&cwd)).unwrap();

        fs
    }

    #[must_use]
    pub fn snapshot(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }

    /// Writes the files to the file system.
    ///
    /// The operation overrides existing files with the same normalized path.
    ///
    /// Enclosing directories are automatically created if they don't exist.
    pub fn write_files<P, C>(&self, files: impl IntoIterator<Item = (P, C)>) -> Result<()>
    where
        P: AsRef<FileSystemPath>,
        C: ToString,
    {
        for (path, content) in files {
            self.write_file(path.as_ref(), content.to_string())?;
        }

        Ok(())
    }

    /// Stores a new file in the file system.
    ///
    /// The operation overrides the content for an existing file with the same normalized `path`.
    ///
    /// Enclosing directories are automatically created if they don't exist.
    pub fn write_file(&self, path: impl AsRef<FileSystemPath>, content: String) -> Result<()> {
        let mut by_path = self.inner.by_path.write().unwrap();

        let normalized = normalize_path(path.as_ref(), &self.inner.cwd);

        get_or_create_file(&mut by_path, &normalized)?.content = content;

        Ok(())
    }

    /// Sets the last modified time stamp of the file stored at `path` to now.
    ///
    /// Creates a new file if the file at `path` doesn't exist.
    pub fn touch(&self, path: impl AsRef<FileSystemPath>) -> Result<()> {
        let mut by_path = self.inner.by_path.write().unwrap();
        let normalized = normalize_path(path.as_ref(), &self.inner.cwd);

        get_or_create_file(&mut by_path, &normalized)?.last_modified = FileTime::now();

        Ok(())
    }

    /// Creates a directory at `path`. All enclosing directories are created if they don't exist.
    pub fn create_directory_all(&self, path: impl AsRef<FileSystemPath>) -> Result<()> {
        let mut by_path = self.inner.by_path.write().unwrap();
        let normalized = normalize_path(path.as_ref(), &self.inner.cwd);

        create_dir_all(&mut by_path, &normalized)
    }
}

impl FileSystem for MemoryFileSystem {
    fn metadata(&self, path: &FileSystemPath) -> Result<Metadata> {
        let by_path = self.inner.by_path.read().unwrap();
        let normalized = normalize_path(path, &self.inner.cwd);

        let entry = by_path.get(&normalized).ok_or_else(not_found)?;

        let metadata = match entry {
            Entry::File(file) => Metadata {
                revision: file.last_modified.into(),
                permissions: Some(Self::PERMISSION),
                file_type: FileType::File,
            },
            Entry::Directory(directory) => Metadata {
                revision: directory.last_modified.into(),
                permissions: Some(Self::PERMISSION),
                file_type: FileType::Directory,
            },
        };

        Ok(metadata)
    }

    fn read(&self, path: &FileSystemPath) -> Result<String> {
        let by_path = self.inner.by_path.read().unwrap();
        let normalized = normalize_path(path, &self.inner.cwd);

        let entry = by_path.get(&normalized).ok_or_else(not_found)?;

        match entry {
            Entry::File(file) => Ok(file.content.clone()),
            Entry::Directory(_) => Err(is_a_directory()),
        }
    }

    fn exists(&self, path: &FileSystemPath) -> bool {
        let by_path = self.inner.by_path.read().unwrap();
        let normalized = normalize_path(path, &self.inner.cwd);

        by_path.contains_key(&normalized)
    }
}

impl Default for MemoryFileSystem {
    fn default() -> Self {
        MemoryFileSystem::new()
    }
}

impl std::fmt::Debug for MemoryFileSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let paths = self.inner.by_path.read().unwrap();

        f.debug_map().entries(paths.iter()).finish()
    }
}

struct MemoryFileSystemInner {
    by_path: RwLock<FxHashMap<Utf8PathBuf, Entry>>,
    cwd: Utf8PathBuf,
}

#[derive(Debug)]
enum Entry {
    File(File),
    Directory(Directory),
}

impl Entry {
    const fn is_file(&self) -> bool {
        matches!(self, Entry::File(_))
    }
}

#[derive(Debug)]
struct File {
    content: String,
    last_modified: FileTime,
}

#[derive(Debug)]
struct Directory {
    last_modified: FileTime,
}

fn not_found() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory")
}

fn is_a_directory() -> std::io::Error {
    // Note: Rust returns `ErrorKind::IsADirectory` for this error but this is a nightly only variant :(.
    //   So we have to use other for now.
    std::io::Error::new(std::io::ErrorKind::Other, "Is a directory")
}

fn not_a_directory() -> std::io::Error {
    // Note: Rust returns `ErrorKind::NotADirectory` for this error but this is a nightly only variant :(.
    //   So we have to use `Other` for now.
    std::io::Error::new(std::io::ErrorKind::Other, "Not a directory")
}

/// Normalizes the path by removing `.` and `..` components and transform the path into an absolute path.
///
/// Adapted from https://github.com/rust-lang/cargo/blob/fede83ccf973457de319ba6fa0e36ead454d2e20/src/cargo/util/paths.rs#L61
fn normalize_path(path: &FileSystemPath, cwd: &Utf8Path) -> Utf8PathBuf {
    let path = camino::Utf8Path::new(path.as_str());

    let mut components = path.components().peekable();
    let mut ret =
        if let Some(c @ camino::Utf8Component::Prefix(..) | c @ camino::Utf8Component::RootDir) =
            components.peek().cloned()
        {
            components.next();
            Utf8PathBuf::from(c.as_str())
        } else {
            cwd.to_path_buf()
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

    ret
}

fn create_dir_all(
    paths: &mut RwLockWriteGuard<FxHashMap<Utf8PathBuf, Entry>>,
    normalized: &Utf8Path,
) -> Result<()> {
    let mut path = Utf8PathBuf::new();

    for component in normalized.components() {
        path.push(component);
        let entry = paths.entry(path.clone()).or_insert_with(|| {
            Entry::Directory(Directory {
                last_modified: FileTime::now(),
            })
        });

        if entry.is_file() {
            return Err(not_a_directory());
        }
    }

    Ok(())
}

fn get_or_create_file<'a>(
    paths: &'a mut RwLockWriteGuard<FxHashMap<Utf8PathBuf, Entry>>,
    normalized: &Utf8Path,
) -> Result<&'a mut File> {
    if let Some(parent) = normalized.parent() {
        create_dir_all(paths, parent)?;
    }

    let entry = paths.entry(normalized.to_path_buf()).or_insert_with(|| {
        Entry::File(File {
            content: String::new(),
            last_modified: FileTime::now(),
        })
    });

    match entry {
        Entry::File(file) => Ok(file),
        Entry::Directory(_) => Err(is_a_directory()),
    }
}

#[cfg(test)]
mod tests {
    use crate::file_system::{FileSystem, FileSystemPath, MemoryFileSystem, Result};
    use std::io::ErrorKind;
    use std::time::Duration;

    /// Creates a file system with the given files.
    ///
    /// The content of all files will be empty.
    fn with_files<P>(files: impl IntoIterator<Item = P>) -> super::MemoryFileSystem
    where
        P: AsRef<FileSystemPath>,
    {
        let fs = MemoryFileSystem::new();
        fs.write_files(files.into_iter().map(|path| (path, "")))
            .unwrap();

        fs
    }

    #[test]
    fn is_file() {
        let path = FileSystemPath::new("a.py");
        let fs = with_files([path]);

        assert!(fs.is_file(path));
        assert!(!fs.is_directory(path));
    }

    #[test]
    fn exists() {
        let fs = with_files(["a.py"]);

        assert!(fs.exists(FileSystemPath::new("a.py")));
        assert!(!fs.exists(FileSystemPath::new("b.py")));
    }

    #[test]
    fn exists_directories() {
        let fs = with_files(["a/b/c.py"]);

        assert!(fs.exists(FileSystemPath::new("a")));
        assert!(fs.exists(FileSystemPath::new("a/b")));
        assert!(fs.exists(FileSystemPath::new("a/b/c.py")));
    }

    #[test]
    fn path_normalization() {
        let fs = with_files(["a.py"]);

        assert!(fs.exists(FileSystemPath::new("a.py")));
        assert!(fs.exists(FileSystemPath::new("/a.py")));
        assert!(fs.exists(FileSystemPath::new("/b/./../a.py")));
    }

    #[test]
    fn permissions() -> Result<()> {
        let fs = with_files(["a.py"]);

        // The default permissions match the default on Linux: 0755
        assert_eq!(
            fs.metadata(FileSystemPath::new("a.py"))?.permissions(),
            Some(MemoryFileSystem::PERMISSION)
        );

        Ok(())
    }

    #[test]
    fn touch() -> Result<()> {
        let fs = MemoryFileSystem::new();
        let path = FileSystemPath::new("a.py");

        // Creates a file if it doesn't exist
        fs.touch(path)?;

        assert!(fs.exists(path));

        let timestamp1 = fs.metadata(path)?.revision();

        // Sleep to ensure that the timestamp changes
        std::thread::sleep(Duration::from_millis(1));

        fs.touch(path)?;

        let timestamp2 = fs.metadata(path)?.revision();

        assert_ne!(timestamp1, timestamp2);

        Ok(())
    }

    #[test]
    fn create_dir_all() {
        let fs = MemoryFileSystem::new();

        fs.create_directory_all(FileSystemPath::new("a/b/c"))
            .unwrap();

        assert!(fs.is_directory(FileSystemPath::new("a")));
        assert!(fs.is_directory(FileSystemPath::new("a/b")));
        assert!(fs.is_directory(FileSystemPath::new("a/b/c")));

        // Should not fail if the directory already exists
        fs.create_directory_all(FileSystemPath::new("a/b/c"))
            .unwrap();
    }

    #[test]
    fn create_dir_all_fails_if_a_component_is_a_file() {
        let fs = with_files(["a/b.py"]);

        let error = fs
            .create_directory_all(FileSystemPath::new("a/b.py/c"))
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Other);
    }

    #[test]
    fn write_file_fails_if_a_component_is_a_file() {
        let fs = with_files(["a/b.py"]);

        let error = fs
            .write_file(FileSystemPath::new("a/b.py/c"), "content".to_string())
            .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::Other);
    }

    #[test]
    fn write_file_fails_if_path_points_to_a_directory() -> Result<()> {
        let fs = MemoryFileSystem::new();

        fs.create_directory_all("a")?;

        let error = fs
            .write_file(FileSystemPath::new("a"), "content".to_string())
            .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::Other);

        Ok(())
    }

    #[test]
    fn read() -> Result<()> {
        let fs = MemoryFileSystem::new();
        let path = FileSystemPath::new("a.py");

        fs.write_file(path, "Test content".to_string())?;

        assert_eq!(fs.read(path)?, "Test content");

        Ok(())
    }

    #[test]
    fn read_fails_if_path_is_a_directory() -> Result<()> {
        let fs = MemoryFileSystem::new();

        fs.create_directory_all("a")?;

        let error = fs.read(FileSystemPath::new("a")).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::Other);

        Ok(())
    }

    #[test]
    fn read_fails_if_path_doesnt_exist() -> Result<()> {
        let fs = MemoryFileSystem::new();

        let error = fs.read(FileSystemPath::new("a")).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::NotFound);

        Ok(())
    }
}
