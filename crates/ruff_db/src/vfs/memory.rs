use std::fmt::Formatter;
use std::sync::Arc;

use filetime::FileTime;

use crate::vfs::{FileRevision, Vfs, VfsError, VfsMetadata, VfsPath, VfsResult};
use crate::FxDashMap;

/// File system that stores all content in memory.
///
/// It's primary use is for testing.
#[derive(Default)]
pub struct MemoryFs {
    by_path: Arc<FxDashMap<VfsPath, FileData>>,
}

impl MemoryFs {
    pub fn snapshot(&self) -> Self {
        Self {
            by_path: self.by_path.clone(),
        }
    }

    pub fn write_files<I>(&self, files: I)
    where
        I: IntoIterator<Item = (VfsPath, String)>,
    {
        for (path, content) in files {
            self.write_file(&path, content);
        }
    }

    /// Updates the content of the file stored at `path`.
    pub fn write_file(&self, path: &VfsPath, content: String) {
        match self.by_path.entry(path.clone()) {
            dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                let data = entry.get_mut();

                data.data = content;
                data.revision = FileRevision::from(FileTime::now());
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(FileData {
                    data: content,
                    revision: FileRevision::from(FileTime::now()),
                    permission: 755,
                });
            }
        }
    }

    /// Sets the revision of the file stored at `path`.
    ///
    /// Creates a new empty file, if the file at `path` doesn't exist (similar to `touch`)
    pub fn set_revision(&self, path: &VfsPath, revision: FileRevision) {
        let mut entry = self
            .by_path
            .entry(path.clone())
            .or_insert_with(|| FileData {
                data: String::new(),
                revision,
                permission: 755,
            });

        entry.value_mut().revision = revision;
    }

    /// Sets the revision of the file stored at `path`.
    ///
    /// Creates an empty file, if the file at `path` doesn't exist.
    pub fn set_permission(&self, path: &VfsPath, permission: u32) {
        let mut entry = self
            .by_path
            .entry(path.clone())
            .or_insert_with(|| FileData {
                data: String::new(),
                revision: FileRevision::zero(),
                permission,
            });

        entry.value_mut().permission = permission;
    }
}

impl Vfs for MemoryFs {
    fn metadata(&self, path: &VfsPath) -> VfsResult<VfsMetadata> {
        let entry = self
            .by_path
            .get(path)
            .ok_or(VfsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found: {:?}", path),
            )))?;

        let data = entry.value();

        Ok(VfsMetadata {
            revision: data.revision,
            permission: data.permission,
        })
    }

    fn exists(&self, path: &VfsPath) -> bool {
        self.by_path.contains_key(path)
    }

    fn read(&self, path: &VfsPath) -> VfsResult<String> {
        self.by_path
            .get(path)
            .map(|data| data.data.clone())
            .ok_or_else(|| {
                VfsError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("File not found: {:?}", path),
                ))
            })
    }
}

impl std::fmt::Debug for MemoryFs {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();

        for entry in self.by_path.iter() {
            map.key(&entry.key()).value(&entry.value());
        }

        map.finish()
    }
}

#[derive(Debug)]
struct FileData {
    data: String,
    revision: FileRevision,
    permission: u32,
}
