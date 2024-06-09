use crate::file_system::{
    FileSystem, FileSystemPath, FileSystemPathBuf, FileType, Metadata, Result,
};
use crate::FxDashMap;
use dashmap::mapref::one::RefMut;
use filetime::FileTime;
use rustc_hash::FxHasher;
use std::hash::BuildHasherDefault;
use std::io::ErrorKind;
use std::sync::Arc;

/// In memory file system.
///
/// Only intended for testing purposes. Directories aren't yet supported.
#[derive(Default)]
pub struct MemoryFileSystem {
    inner: Arc<MemoryFileSystemInner>,
}

impl MemoryFileSystem {
    pub fn snapshot(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }

    /// Writes the files to the file system.
    pub fn write_files<P, C>(&self, files: impl IntoIterator<Item = (P, C)>)
    where
        P: AsRef<FileSystemPath>,
        C: ToString,
    {
        for (path, content) in files {
            self.write_file(path.as_ref(), content.to_string());
        }
    }

    /// Stores a new file in the file system
    pub fn write_file(&self, path: &FileSystemPath, content: String) {
        let mut entry = self.entry_or_insert(path);
        let value = entry.value_mut();

        value.content = content;
        value.last_modified = FileTime::now();
    }

    /// Sets the permissions of the file at `path`.
    ///
    /// Creates a new file with an empty content if the file doesn't exist.
    pub fn set_permissions(&self, path: &FileSystemPath, permissions: u32) {
        let mut entry = self.entry_or_insert(path);
        let value = entry.value_mut();
        value.permission = permissions;
    }

    /// Updates the last modified time of the file at `path` to now.
    ///
    /// Creates a new file with an empty content if the file doesn't exist.
    pub fn touch(&self, path: &FileSystemPath) {
        let mut entry = self.entry_or_insert(path);
        let value = entry.value_mut();

        value.last_modified = FileTime::now();
    }

    fn entry_or_insert(
        &self,
        path: &FileSystemPath,
    ) -> RefMut<FileSystemPathBuf, FileData, BuildHasherDefault<FxHasher>> {
        self.inner
            .files
            .entry(path.to_path_buf())
            .or_insert_with(|| FileData {
                content: String::new(),
                last_modified: FileTime::now(),
                permission: 0o755,
            })
    }
}

impl FileSystem for MemoryFileSystem {
    fn metadata(&self, path: &FileSystemPath) -> Result<Metadata> {
        let entry = self
            .inner
            .files
            .get(&path.to_path_buf())
            .ok_or_else(|| std::io::Error::new(ErrorKind::NotFound, "File not found"))?;

        let value = entry.value();

        Ok(Metadata {
            revision: value.last_modified.into(),
            permissions: Some(value.permission),
            file_type: FileType::File,
        })
    }

    fn read(&self, path: &FileSystemPath) -> Result<String> {
        let entry = self
            .inner
            .files
            .get(&path.to_path_buf())
            .ok_or_else(|| std::io::Error::new(ErrorKind::NotFound, "File not found"))?;

        let value = entry.value();

        Ok(value.content.clone())
    }

    fn exists(&self, path: &FileSystemPath) -> bool {
        self.inner.files.contains_key(&path.to_path_buf())
    }
}

impl std::fmt::Debug for MemoryFileSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();

        for entry in self.inner.files.iter() {
            map.entry(entry.key(), entry.value());
        }
        map.finish()
    }
}

#[derive(Default)]
struct MemoryFileSystemInner {
    files: FxDashMap<FileSystemPathBuf, FileData>,
}

#[derive(Debug)]
struct FileData {
    content: String,
    last_modified: FileTime,
    permission: u32,
}
