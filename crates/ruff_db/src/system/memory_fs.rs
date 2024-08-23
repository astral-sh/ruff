use std::collections::BTreeMap;
use std::iter::FusedIterator;
use std::sync::{Arc, RwLock, RwLockWriteGuard};

use camino::{Utf8Path, Utf8PathBuf};
use filetime::FileTime;
use rustc_hash::FxHashMap;

use ruff_notebook::{Notebook, NotebookError};

use crate::system::{
    walk_directory, DirectoryEntry, FileType, Metadata, Result, SystemPath, SystemPathBuf,
    SystemVirtualPath, SystemVirtualPathBuf,
};

use super::walk_directory::{
    DirectoryWalker, WalkDirectoryBuilder, WalkDirectoryConfiguration, WalkDirectoryVisitor,
    WalkDirectoryVisitorBuilder, WalkState,
};

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
/// Use a tempdir with the real file system to test these advanced file system features and behavior.
#[derive(Clone)]
pub struct MemoryFileSystem {
    inner: Arc<MemoryFileSystemInner>,
}

impl MemoryFileSystem {
    /// Permission used by all files and directories
    const PERMISSION: u32 = 0o755;

    pub fn new() -> Self {
        Self::with_current_directory("/")
    }

    /// Creates a new, empty in memory file system with the given current working directory.
    pub fn with_current_directory(cwd: impl AsRef<SystemPath>) -> Self {
        let cwd = cwd.as_ref().to_path_buf();

        assert!(
            cwd.starts_with("/"),
            "The current working directory must be an absolute path."
        );

        let fs = Self {
            inner: Arc::new(MemoryFileSystemInner {
                by_path: RwLock::new(BTreeMap::default()),
                virtual_files: RwLock::new(FxHashMap::default()),
                cwd: cwd.clone(),
            }),
        };

        fs.create_directory_all(&cwd).unwrap();

        fs
    }

    pub fn current_directory(&self) -> &SystemPath {
        &self.inner.cwd
    }

    pub fn metadata(&self, path: impl AsRef<SystemPath>) -> Result<Metadata> {
        fn metadata(fs: &MemoryFileSystem, path: &SystemPath) -> Result<Metadata> {
            let by_path = fs.inner.by_path.read().unwrap();
            let normalized = fs.normalize_path(path);

            let entry = by_path.get(&normalized).ok_or_else(not_found)?;

            let metadata = match entry {
                Entry::File(file) => Metadata {
                    revision: file.last_modified.into(),
                    permissions: Some(MemoryFileSystem::PERMISSION),
                    file_type: FileType::File,
                },
                Entry::Directory(directory) => Metadata {
                    revision: directory.last_modified.into(),
                    permissions: Some(MemoryFileSystem::PERMISSION),
                    file_type: FileType::Directory,
                },
            };

            Ok(metadata)
        }

        metadata(self, path.as_ref())
    }

    pub fn canonicalize(&self, path: impl AsRef<SystemPath>) -> SystemPathBuf {
        SystemPathBuf::from_utf8_path_buf(self.normalize_path(path))
    }

    pub fn is_file(&self, path: impl AsRef<SystemPath>) -> bool {
        let by_path = self.inner.by_path.read().unwrap();
        let normalized = self.normalize_path(path.as_ref());

        matches!(by_path.get(&normalized), Some(Entry::File(_)))
    }

    pub fn is_directory(&self, path: impl AsRef<SystemPath>) -> bool {
        let by_path = self.inner.by_path.read().unwrap();
        let normalized = self.normalize_path(path.as_ref());

        matches!(by_path.get(&normalized), Some(Entry::Directory(_)))
    }

    pub fn read_to_string(&self, path: impl AsRef<SystemPath>) -> Result<String> {
        fn read_to_string(fs: &MemoryFileSystem, path: &SystemPath) -> Result<String> {
            let by_path = fs.inner.by_path.read().unwrap();
            let normalized = fs.normalize_path(path);

            let entry = by_path.get(&normalized).ok_or_else(not_found)?;

            match entry {
                Entry::File(file) => Ok(file.content.clone()),
                Entry::Directory(_) => Err(is_a_directory()),
            }
        }

        read_to_string(self, path.as_ref())
    }

    pub(crate) fn read_to_notebook(
        &self,
        path: impl AsRef<SystemPath>,
    ) -> std::result::Result<ruff_notebook::Notebook, ruff_notebook::NotebookError> {
        let content = self.read_to_string(path)?;
        ruff_notebook::Notebook::from_source_code(&content)
    }

    pub(crate) fn read_virtual_path_to_string(
        &self,
        path: impl AsRef<SystemVirtualPath>,
    ) -> Result<String> {
        let virtual_files = self.inner.virtual_files.read().unwrap();
        let file = virtual_files
            .get(&path.as_ref().to_path_buf())
            .ok_or_else(not_found)?;

        Ok(file.content.clone())
    }

    pub(crate) fn read_virtual_path_to_notebook(
        &self,
        path: &SystemVirtualPath,
    ) -> std::result::Result<Notebook, NotebookError> {
        let content = self.read_virtual_path_to_string(path)?;
        ruff_notebook::Notebook::from_source_code(&content)
    }

    pub fn exists(&self, path: &SystemPath) -> bool {
        let by_path = self.inner.by_path.read().unwrap();
        let normalized = self.normalize_path(path);

        by_path.contains_key(&normalized)
    }

    pub fn virtual_path_exists(&self, path: &SystemVirtualPath) -> bool {
        let virtual_files = self.inner.virtual_files.read().unwrap();
        virtual_files.contains_key(&path.to_path_buf())
    }

    /// Writes the files to the file system.
    ///
    /// The operation overrides existing files with the same normalized path.
    ///
    /// Enclosing directories are automatically created if they don't exist.
    pub fn write_files<P, C>(&self, files: impl IntoIterator<Item = (P, C)>) -> Result<()>
    where
        P: AsRef<SystemPath>,
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
    pub fn write_file(&self, path: impl AsRef<SystemPath>, content: impl ToString) -> Result<()> {
        let mut by_path = self.inner.by_path.write().unwrap();

        let normalized = self.normalize_path(path.as_ref());

        let file = get_or_create_file(&mut by_path, &normalized)?;
        file.content = content.to_string();
        file.last_modified = now();

        Ok(())
    }

    /// Stores a new virtual file in the file system.
    ///
    /// The operation overrides the content for an existing virtual file with the same `path`.
    pub fn write_virtual_file(&self, path: impl AsRef<SystemVirtualPath>, content: impl ToString) {
        let path = path.as_ref();
        let mut virtual_files = self.inner.virtual_files.write().unwrap();

        match virtual_files.entry(path.to_path_buf()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(File {
                    content: content.to_string(),
                    last_modified: now(),
                });
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                entry.get_mut().content = content.to_string();
            }
        }
    }

    /// Returns a builder for walking the directory tree of `path`.
    ///
    /// The only files that are ignored when setting `WalkDirectoryBuilder::standard_filters`
    /// are hidden files (files with a name starting with a `.`).
    pub fn walk_directory(&self, path: impl AsRef<SystemPath>) -> WalkDirectoryBuilder {
        WalkDirectoryBuilder::new(path, MemoryWalker { fs: self.clone() })
    }

    pub fn remove_file(&self, path: impl AsRef<SystemPath>) -> Result<()> {
        fn remove_file(fs: &MemoryFileSystem, path: &SystemPath) -> Result<()> {
            let mut by_path = fs.inner.by_path.write().unwrap();
            let normalized = fs.normalize_path(path);

            match by_path.entry(normalized) {
                std::collections::btree_map::Entry::Occupied(entry) => match entry.get() {
                    Entry::File(_) => {
                        entry.remove();
                        Ok(())
                    }
                    Entry::Directory(_) => Err(is_a_directory()),
                },
                std::collections::btree_map::Entry::Vacant(_) => Err(not_found()),
            }
        }

        remove_file(self, path.as_ref())
    }

    pub fn remove_virtual_file(&self, path: impl AsRef<SystemVirtualPath>) -> Result<()> {
        let mut virtual_files = self.inner.virtual_files.write().unwrap();
        match virtual_files.entry(path.as_ref().to_path_buf()) {
            std::collections::hash_map::Entry::Occupied(entry) => {
                entry.remove();
                Ok(())
            }
            std::collections::hash_map::Entry::Vacant(_) => Err(not_found()),
        }
    }

    /// Sets the last modified timestamp of the file stored at `path` to now.
    ///
    /// Creates a new file if the file at `path` doesn't exist.
    pub fn touch(&self, path: impl AsRef<SystemPath>) -> Result<()> {
        let mut by_path = self.inner.by_path.write().unwrap();
        let normalized = self.normalize_path(path.as_ref());

        get_or_create_file(&mut by_path, &normalized)?.last_modified = now();

        Ok(())
    }

    /// Creates a directory at `path`. All enclosing directories are created if they don't exist.
    pub fn create_directory_all(&self, path: impl AsRef<SystemPath>) -> Result<()> {
        let mut by_path = self.inner.by_path.write().unwrap();
        let normalized = self.normalize_path(path.as_ref());

        create_dir_all(&mut by_path, &normalized)
    }

    /// Deletes the directory at `path`.
    ///
    /// ## Errors
    /// * If the directory is not empty
    /// * The `path` is not a directory
    /// * The `path` does not exist
    pub fn remove_directory(&self, path: impl AsRef<SystemPath>) -> Result<()> {
        fn remove_directory(fs: &MemoryFileSystem, path: &SystemPath) -> Result<()> {
            let mut by_path = fs.inner.by_path.write().unwrap();
            let normalized = fs.normalize_path(path);

            // Test if the directory is empty
            // Skip the directory path itself
            for (maybe_child, _) in by_path.range(normalized.clone()..).skip(1) {
                if maybe_child.starts_with(&normalized) {
                    return Err(directory_not_empty());
                } else if !maybe_child.as_str().starts_with(normalized.as_str()) {
                    break;
                }
            }

            match by_path.entry(normalized.clone()) {
                std::collections::btree_map::Entry::Occupied(entry) => match entry.get() {
                    Entry::Directory(_) => {
                        entry.remove();
                        Ok(())
                    }
                    Entry::File(_) => Err(not_a_directory()),
                },
                std::collections::btree_map::Entry::Vacant(_) => Err(not_found()),
            }
        }

        remove_directory(self, path.as_ref())
    }

    fn normalize_path(&self, path: impl AsRef<SystemPath>) -> Utf8PathBuf {
        let normalized = SystemPath::absolute(path, &self.inner.cwd);
        normalized.into_utf8_path_buf()
    }

    pub fn read_directory(&self, path: impl AsRef<SystemPath>) -> Result<ReadDirectory> {
        let by_path = self.inner.by_path.read().unwrap();
        let normalized = self.normalize_path(path.as_ref());
        let entry = by_path.get(&normalized).ok_or_else(not_found)?;
        if entry.is_file() {
            return Err(not_a_directory());
        };

        // Collect the entries into a vector to avoid deadlocks when the
        // consumer calls into other file system methods while iterating over the
        // directory entries.
        let collected = by_path
            .range(normalized.clone()..)
            .skip(1)
            .take_while(|(path, _)| path.starts_with(&normalized))
            .filter_map(|(path, entry)| {
                if path.parent()? == normalized {
                    Some(Ok(DirectoryEntry {
                        path: SystemPathBuf::from_utf8_path_buf(path.to_owned()),
                        file_type: entry.file_type(),
                    }))
                } else {
                    None
                }
            })
            .collect();

        Ok(ReadDirectory::new(collected))
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
    by_path: RwLock<BTreeMap<Utf8PathBuf, Entry>>,
    virtual_files: RwLock<FxHashMap<SystemVirtualPathBuf, File>>,
    cwd: SystemPathBuf,
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

    const fn file_type(&self) -> FileType {
        match self {
            Self::File(_) => FileType::File,
            Self::Directory(_) => FileType::Directory,
        }
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

fn directory_not_empty() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, "directory not empty")
}

fn create_dir_all(
    paths: &mut RwLockWriteGuard<BTreeMap<Utf8PathBuf, Entry>>,
    normalized: &Utf8Path,
) -> Result<()> {
    let mut path = Utf8PathBuf::new();

    for component in normalized.components() {
        path.push(component);
        let entry = paths.entry(path.clone()).or_insert_with(|| {
            Entry::Directory(Directory {
                last_modified: now(),
            })
        });

        if entry.is_file() {
            return Err(not_a_directory());
        }
    }

    Ok(())
}

fn get_or_create_file<'a>(
    paths: &'a mut RwLockWriteGuard<BTreeMap<Utf8PathBuf, Entry>>,
    normalized: &Utf8Path,
) -> Result<&'a mut File> {
    if let Some(parent) = normalized.parent() {
        create_dir_all(paths, parent)?;
    }

    let entry = paths.entry(normalized.to_path_buf()).or_insert_with(|| {
        Entry::File(File {
            content: String::new(),
            last_modified: now(),
        })
    });

    match entry {
        Entry::File(file) => Ok(file),
        Entry::Directory(_) => Err(is_a_directory()),
    }
}

#[derive(Debug)]
pub struct ReadDirectory {
    entries: std::vec::IntoIter<Result<DirectoryEntry>>,
}

impl ReadDirectory {
    fn new(entries: Vec<Result<DirectoryEntry>>) -> Self {
        Self {
            entries: entries.into_iter(),
        }
    }
}

impl Iterator for ReadDirectory {
    type Item = std::io::Result<DirectoryEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next()
    }
}

impl FusedIterator for ReadDirectory {}

/// Recursively walks a directory in the memory file system.
#[derive(Debug)]
struct MemoryWalker {
    fs: MemoryFileSystem,
}

impl MemoryWalker {
    fn visit_entry(
        &self,
        visitor: &mut dyn WalkDirectoryVisitor,
        entry: walk_directory::DirectoryEntry,
        queue: &mut Vec<WalkerState>,
        ignore_hidden: bool,
    ) -> WalkState {
        if entry.file_type().is_directory() {
            let path = entry.path.clone();
            let depth = entry.depth;

            let state = visitor.visit(Ok(entry));

            if matches!(state, WalkState::Continue) {
                queue.push(WalkerState::Nested {
                    path,
                    depth: depth + 1,
                });
            }

            state
        } else if ignore_hidden
            && entry
                .path
                .file_name()
                .is_some_and(|name| name.starts_with('.'))
        {
            WalkState::Skip
        } else {
            visitor.visit(Ok(entry))
        }
    }
}

impl DirectoryWalker for MemoryWalker {
    fn walk(
        &self,
        builder: &mut dyn WalkDirectoryVisitorBuilder,
        configuration: WalkDirectoryConfiguration,
    ) {
        let WalkDirectoryConfiguration {
            paths,
            ignore_hidden,
            standard_filters: _,
        } = configuration;

        let mut visitor = builder.build();
        let mut queue: Vec<_> = paths
            .into_iter()
            .map(|path| WalkerState::Start { path })
            .collect();

        while let Some(state) = queue.pop() {
            let (path, depth) = match state {
                WalkerState::Start { path } => {
                    match self.fs.metadata(&path) {
                        Ok(metadata) => {
                            let entry = walk_directory::DirectoryEntry {
                                file_type: metadata.file_type,
                                depth: 0,
                                path,
                            };

                            if self.visit_entry(&mut *visitor, entry, &mut queue, ignore_hidden)
                                == WalkState::Quit
                            {
                                return;
                            }
                        }
                        Err(error) => {
                            visitor.visit(Err(walk_directory::Error {
                                depth: Some(0),
                                kind: walk_directory::ErrorKind::Io {
                                    path: Some(path),
                                    err: error,
                                },
                            }));
                        }
                    }

                    continue;
                }
                WalkerState::Nested { path, depth } => (path, depth),
            };

            // Use `read_directory` here instead of locking `by_path` to avoid deadlocks
            // when the `visitor` calls any file system operations.
            let entries = match self.fs.read_directory(&path) {
                Ok(entries) => entries,
                Err(error) => {
                    visitor.visit(Err(walk_directory::Error {
                        depth: Some(depth),
                        kind: walk_directory::ErrorKind::Io {
                            path: Some(path),
                            err: error,
                        },
                    }));

                    continue;
                }
            };

            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let entry = walk_directory::DirectoryEntry {
                            file_type: entry.file_type,
                            depth,
                            path: entry.path,
                        };

                        if self.visit_entry(&mut *visitor, entry, &mut queue, ignore_hidden)
                            == WalkState::Quit
                        {
                            return;
                        }
                    }

                    Err(error) => {
                        visitor.visit(Err(walk_directory::Error {
                            depth: Some(depth),
                            kind: walk_directory::ErrorKind::Io {
                                path: None,
                                err: error,
                            },
                        }));
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
enum WalkerState {
    /// An entry path that was directly provided to the walker. Always has depth 0.
    Start { path: SystemPathBuf },

    /// Traverse into the directory with the given path at the given depth.
    Nested { path: SystemPathBuf, depth: usize },
}

#[cfg(not(target_arch = "wasm32"))]
fn now() -> FileTime {
    FileTime::now()
}

#[cfg(target_arch = "wasm32")]
fn now() -> FileTime {
    // Copied from FileTime::from_system_time()
    let time = web_time::SystemTime::now();

    time.duration_since(web_time::UNIX_EPOCH)
        .map(|d| FileTime::from_unix_time(d.as_secs() as i64, d.subsec_nanos()))
        .unwrap_or_else(|e| {
            let until_epoch = e.duration();
            let (sec_offset, nanos) = if until_epoch.subsec_nanos() == 0 {
                (0, 0)
            } else {
                (-1, 1_000_000_000 - until_epoch.subsec_nanos())
            };

            FileTime::from_unix_time(-(until_epoch.as_secs() as i64) + sec_offset, nanos)
        })
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::time::Duration;

    use crate::system::walk_directory::tests::DirectoryEntryToString;
    use crate::system::walk_directory::WalkState;
    use crate::system::{
        DirectoryEntry, FileType, MemoryFileSystem, Result, SystemPath, SystemPathBuf,
        SystemVirtualPath,
    };

    /// Creates a file system with the given files.
    ///
    /// The content of all files will be empty.
    fn with_files<P>(files: impl IntoIterator<Item = P>) -> super::MemoryFileSystem
    where
        P: AsRef<SystemPath>,
    {
        let fs = MemoryFileSystem::new();
        fs.write_files(files.into_iter().map(|path| (path, "")))
            .unwrap();

        fs
    }

    #[test]
    fn is_file() {
        let path = SystemPath::new("a.py");
        let fs = with_files([path]);

        assert!(fs.is_file(path));
        assert!(!fs.is_directory(path));
    }

    #[test]
    fn exists() {
        let fs = with_files(["a.py"]);

        assert!(fs.exists(SystemPath::new("a.py")));
        assert!(!fs.exists(SystemPath::new("b.py")));
    }

    #[test]
    fn exists_directories() {
        let fs = with_files(["a/b/c.py"]);

        assert!(fs.exists(SystemPath::new("a")));
        assert!(fs.exists(SystemPath::new("a/b")));
        assert!(fs.exists(SystemPath::new("a/b/c.py")));
    }

    #[test]
    fn path_normalization() {
        let fs = with_files(["a.py"]);

        assert!(fs.exists(SystemPath::new("a.py")));
        assert!(fs.exists(SystemPath::new("/a.py")));
        assert!(fs.exists(SystemPath::new("/b/./../a.py")));
    }

    #[test]
    fn permissions() -> Result<()> {
        let fs = with_files(["a.py"]);

        // The default permissions match the default on Linux: 0755
        assert_eq!(
            fs.metadata(SystemPath::new("a.py"))?.permissions(),
            Some(MemoryFileSystem::PERMISSION)
        );

        Ok(())
    }

    #[test]
    fn touch() -> Result<()> {
        let fs = MemoryFileSystem::new();
        let path = SystemPath::new("a.py");

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

        fs.create_directory_all(SystemPath::new("a/b/c")).unwrap();

        assert!(fs.is_directory(SystemPath::new("a")));
        assert!(fs.is_directory(SystemPath::new("a/b")));
        assert!(fs.is_directory(SystemPath::new("a/b/c")));

        // Should not fail if the directory already exists
        fs.create_directory_all(SystemPath::new("a/b/c")).unwrap();
    }

    #[test]
    fn create_dir_all_fails_if_a_component_is_a_file() {
        let fs = with_files(["a/b.py"]);

        let error = fs
            .create_directory_all(SystemPath::new("a/b.py/c"))
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Other);
    }

    #[test]
    fn write_file_fails_if_a_component_is_a_file() {
        let fs = with_files(["a/b.py"]);

        let error = fs
            .write_file(SystemPath::new("a/b.py/c"), "content".to_string())
            .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::Other);
    }

    #[test]
    fn write_file_fails_if_path_points_to_a_directory() -> Result<()> {
        let fs = MemoryFileSystem::new();

        fs.create_directory_all("a")?;

        let error = fs
            .write_file(SystemPath::new("a"), "content".to_string())
            .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::Other);

        Ok(())
    }

    #[test]
    fn write_virtual_file() {
        let fs = MemoryFileSystem::new();

        fs.write_virtual_file("a", "content");

        let error = fs.read_to_string("a").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);

        assert_eq!(fs.read_virtual_path_to_string("a").unwrap(), "content");
    }

    #[test]
    fn read() -> Result<()> {
        let fs = MemoryFileSystem::new();
        let path = SystemPath::new("a.py");

        fs.write_file(path, "Test content".to_string())?;

        assert_eq!(fs.read_to_string(path)?, "Test content");

        Ok(())
    }

    #[test]
    fn read_fails_if_path_is_a_directory() -> Result<()> {
        let fs = MemoryFileSystem::new();

        fs.create_directory_all("a")?;

        let error = fs.read_to_string(SystemPath::new("a")).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::Other);

        Ok(())
    }

    #[test]
    fn read_fails_if_path_doesnt_exist() -> Result<()> {
        let fs = MemoryFileSystem::new();

        let error = fs.read_to_string(SystemPath::new("a")).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::NotFound);

        Ok(())
    }

    #[test]
    fn read_fails_if_virtual_path_doesnt_exit() {
        let fs = MemoryFileSystem::new();

        let error = fs.read_virtual_path_to_string("a").unwrap_err();

        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn remove_file() -> Result<()> {
        let fs = with_files(["a/a.py", "b.py"]);

        fs.remove_file("a/a.py")?;

        assert!(!fs.exists(SystemPath::new("a/a.py")));

        // It doesn't delete the enclosing directories
        assert!(fs.exists(SystemPath::new("a")));

        // It doesn't delete unrelated files.
        assert!(fs.exists(SystemPath::new("b.py")));

        Ok(())
    }

    #[test]
    fn remove_virtual_file() {
        let fs = MemoryFileSystem::new();
        fs.write_virtual_file("a", "content");
        fs.write_virtual_file("b", "content");

        fs.remove_virtual_file("a").unwrap();

        assert!(!fs.virtual_path_exists(SystemVirtualPath::new("a")));
        assert!(fs.virtual_path_exists(SystemVirtualPath::new("b")));
    }

    #[test]
    fn remove_non_existing_file() {
        let fs = with_files(["b.py"]);

        let error = fs.remove_file("a.py").unwrap_err();

        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn remove_file_that_is_a_directory() -> Result<()> {
        let fs = MemoryFileSystem::new();
        fs.create_directory_all("a")?;

        let error = fs.remove_file("a").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Other);

        Ok(())
    }

    #[test]
    fn remove_directory() -> Result<()> {
        let fs = with_files(["b.py"]);
        fs.create_directory_all("a")?;

        fs.remove_directory("a")?;

        assert!(!fs.exists(SystemPath::new("a")));

        // It doesn't delete unrelated files.
        assert!(fs.exists(SystemPath::new("b.py")));

        Ok(())
    }

    #[test]
    fn remove_non_empty_directory() {
        let fs = with_files(["a/a.py"]);

        let error = fs.remove_directory("a").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Other);
    }

    #[test]
    fn remove_directory_with_files_that_start_with_the_same_string() -> Result<()> {
        let fs = with_files(["foo_bar.py", "foob.py"]);
        fs.create_directory_all("foo")?;

        fs.remove_directory("foo").unwrap();

        assert!(!fs.exists(SystemPath::new("foo")));
        assert!(fs.exists(SystemPath::new("foo_bar.py")));
        assert!(fs.exists(SystemPath::new("foob.py")));

        Ok(())
    }

    #[test]
    fn remove_non_existing_directory() {
        let fs = MemoryFileSystem::new();

        let error = fs.remove_directory("a").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn remove_directory_that_is_a_file() {
        let fs = with_files(["a"]);

        let error = fs.remove_directory("a").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Other);
    }

    #[test]
    fn read_directory() {
        let fs = with_files(["b.ts", "a/bar.py", "d.rs", "a/foo/bar.py", "a/baz.pyi"]);
        let contents: Vec<DirectoryEntry> = fs
            .read_directory("a")
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let expected_contents = vec![
            DirectoryEntry::new(SystemPathBuf::from("/a/bar.py"), FileType::File),
            DirectoryEntry::new(SystemPathBuf::from("/a/baz.pyi"), FileType::File),
            DirectoryEntry::new(SystemPathBuf::from("/a/foo"), FileType::Directory),
        ];
        assert_eq!(contents, expected_contents)
    }

    #[test]
    fn read_directory_nonexistent() {
        let fs = MemoryFileSystem::new();
        let Err(error) = fs.read_directory("doesnt_exist") else {
            panic!("Expected this to fail");
        };
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn read_directory_on_file() {
        let fs = with_files(["a.py"]);
        let Err(error) = fs.read_directory("a.py") else {
            panic!("Expected this to fail");
        };
        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert!(error.to_string().contains("Not a directory"));
    }

    #[test]
    fn walk_directory() -> std::io::Result<()> {
        let root = SystemPath::new("/src");
        let system = MemoryFileSystem::with_current_directory(root);

        system.write_files([
            (root.join("foo.py"), "print('foo')"),
            (root.join("a/bar.py"), "print('bar')"),
            (root.join("a/baz.py"), "print('baz')"),
            (root.join("a/b/c.py"), "print('c')"),
        ])?;

        let writer = DirectoryEntryToString::new(root.to_path_buf());

        system.walk_directory(root).run(|| {
            Box::new(|entry| {
                writer.write_entry(entry);

                WalkState::Continue
            })
        });

        assert_eq!(
            writer.to_string(),
            r#"{
    "": (
        Directory,
        0,
    ),
    "a": (
        Directory,
        1,
    ),
    "a/b": (
        Directory,
        2,
    ),
    "a/b/c.py": (
        File,
        3,
    ),
    "a/bar.py": (
        File,
        2,
    ),
    "a/baz.py": (
        File,
        2,
    ),
    "foo.py": (
        File,
        1,
    ),
}"#
        );

        Ok(())
    }

    #[test]
    fn walk_directory_hidden() -> std::io::Result<()> {
        let root = SystemPath::new("/src");
        let system = MemoryFileSystem::with_current_directory(root);

        system.write_files([
            (root.join("foo.py"), "print('foo')"),
            (root.join("a/bar.py"), "print('bar')"),
            (root.join("a/.baz.py"), "print('baz')"),
        ])?;

        let writer = DirectoryEntryToString::new(root.to_path_buf());

        system.walk_directory(root).run(|| {
            Box::new(|entry| {
                writer.write_entry(entry);

                WalkState::Continue
            })
        });

        assert_eq!(
            writer.to_string(),
            r#"{
    "": (
        Directory,
        0,
    ),
    "a": (
        Directory,
        1,
    ),
    "a/bar.py": (
        File,
        2,
    ),
    "foo.py": (
        File,
        1,
    ),
}"#
        );

        Ok(())
    }

    #[test]
    fn walk_directory_file() -> std::io::Result<()> {
        let root = SystemPath::new("/src");
        let system = MemoryFileSystem::with_current_directory(root);

        system.write_file(root.join("foo.py"), "print('foo')")?;

        let writer = DirectoryEntryToString::new(root.to_path_buf());

        system.walk_directory(root.join("foo.py")).run(|| {
            Box::new(|entry| {
                writer.write_entry(entry);

                WalkState::Continue
            })
        });

        assert_eq!(
            writer.to_string(),
            r#"{
    "foo.py": (
        File,
        0,
    ),
}"#
        );

        Ok(())
    }
}
