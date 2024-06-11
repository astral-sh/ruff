use filetime::FileTime;

use crate::file_system::{FileSystem, FileSystemPath, FileType, Metadata, Result};

pub struct OsFileSystem;

impl OsFileSystem {
    #[cfg(unix)]
    fn permissions(metadata: &std::fs::Metadata) -> Option<u32> {
        use std::os::unix::fs::PermissionsExt;

        Some(metadata.permissions().mode())
    }

    #[cfg(not(unix))]
    fn permissions(_metadata: &std::fs::Metadata) -> Option<u32> {
        None
    }

    pub fn snapshot(&self) -> Self {
        Self
    }
}

impl FileSystem for OsFileSystem {
    fn metadata(&self, path: &FileSystemPath) -> Result<Metadata> {
        let metadata = path.as_std_path().metadata()?;
        let last_modified = FileTime::from_last_modification_time(&metadata);

        Ok(Metadata {
            revision: last_modified.into(),
            permissions: Self::permissions(&metadata),
            file_type: metadata.file_type().into(),
        })
    }

    fn read(&self, path: &FileSystemPath) -> Result<String> {
        std::fs::read_to_string(path)
    }

    fn exists(&self, path: &FileSystemPath) -> bool {
        path.as_std_path().exists()
    }
}

impl From<std::fs::FileType> for FileType {
    fn from(file_type: std::fs::FileType) -> Self {
        if file_type.is_file() {
            FileType::File
        } else if file_type.is_dir() {
            FileType::Directory
        } else {
            FileType::Symlink
        }
    }
}
