use crate::system::{FileType, Metadata, Result, System, SystemPath, SystemPathBuf};
use filetime::FileTime;
use std::any::Any;
use std::sync::Arc;

#[derive(Default, Debug)]
pub struct OsSystem {
    inner: Arc<OsSystemInner>,
}

#[derive(Default, Debug)]
struct OsSystemInner {
    cwd: SystemPathBuf,
}

impl OsSystem {
    pub fn new(cwd: impl AsRef<SystemPath>) -> Self {
        Self {
            inner: Arc::new(OsSystemInner {
                cwd: cwd.as_ref().to_path_buf(),
            }),
        }
    }

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
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl System for OsSystem {
    fn path_metadata(&self, path: &SystemPath) -> Result<Metadata> {
        let metadata = path.as_std_path().metadata()?;
        let last_modified = FileTime::from_last_modification_time(&metadata);

        Ok(Metadata {
            revision: last_modified.into(),
            permissions: Self::permissions(&metadata),
            file_type: metadata.file_type().into(),
        })
    }

    fn read_to_string(&self, path: &SystemPath) -> Result<String> {
        std::fs::read_to_string(path.as_std_path())
    }

    fn path_exists(&self, path: &SystemPath) -> bool {
        path.as_std_path().exists()
    }

    fn current_directory(&self) -> &SystemPath {
        &self.inner.cwd
    }

    fn as_any(&self) -> &dyn Any {
        self
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
