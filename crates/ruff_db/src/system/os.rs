use crate::system::{
    DirectoryEntry, FileType, Metadata, Result, System, SystemPath, SystemPathBuf,
};
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

    fn read_directory(
        &self,
        path: &SystemPath,
    ) -> Result<Box<dyn Iterator<Item = Result<DirectoryEntry>>>> {
        Ok(Box::new(
            path.as_utf8_path()
                .read_dir_utf8()?
                .map(|res| res.map(DirectoryEntry::from)),
        ))
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

impl From<camino::Utf8DirEntry> for DirectoryEntry {
    fn from(value: camino::Utf8DirEntry) -> Self {
        let file_type = value.file_type().map(FileType::from);
        Self {
            path: SystemPathBuf::from_utf8_path_buf(value.into_path()),
            file_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn read_directory() {
        let tempdir = TempDir::new().unwrap();
        let tempdir_path = tempdir.path();
        std::fs::create_dir_all(tempdir_path.join("a/foo")).unwrap();
        let files = &["b.ts", "a/bar.py", "d.rs", "a/foo/bar.py", "a/baz.pyi"];
        for path in files {
            std::fs::File::create(tempdir_path.join(path)).unwrap();
        }

        let tempdir_path = SystemPath::from_std_path(tempdir_path).unwrap();
        let fs = OsSystem::new(tempdir_path);

        let mut sorted_contents: Vec<DirectoryEntry> = fs
            .read_directory(&tempdir_path.join("a"))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        sorted_contents.sort_by(|a, b| a.path.cmp(&b.path));

        let expected_contents = vec![
            DirectoryEntry::new(tempdir_path.join("a/bar.py"), Ok(FileType::File)),
            DirectoryEntry::new(tempdir_path.join("a/baz.pyi"), Ok(FileType::File)),
            DirectoryEntry::new(tempdir_path.join("a/foo"), Ok(FileType::Directory)),
        ];
        assert_eq!(sorted_contents, expected_contents)
    }

    #[test]
    fn read_directory_nonexistent() {
        let fs = OsSystem::new("");
        let result = fs.read_directory(SystemPath::new("doesnt_exist"));
        assert!(result.is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound));
    }

    #[test]
    fn read_directory_on_file() {
        let tempdir = TempDir::new().unwrap();
        let tempdir_path = tempdir.path();
        std::fs::File::create(tempdir_path.join("a.py")).unwrap();

        let tempdir_path = SystemPath::from_std_path(tempdir_path).unwrap();
        let fs = OsSystem::new(tempdir_path);
        let result = fs.read_directory(&tempdir_path.join("a.py"));
        let Err(error) = result else {
            panic!("Expected the read_dir() call to fail!");
        };

        // We can't assert the error kind here because it's apparently an unstable feature!
        // https://github.com/rust-lang/rust/issues/86442
        // assert_eq!(error.kind(), std::io::ErrorKind::NotADirectory);

        // We can't even assert the error message on all platforms, as it's different on Windows,
        // where the message is "The directory name is invalid" rather than "Not a directory".
        if cfg!(unix) {
            assert!(error.to_string().contains("Not a directory"));
        }
    }
}
