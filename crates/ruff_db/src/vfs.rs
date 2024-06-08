use std::error::Error;
use std::fmt::Display;

use filetime::FileTime;

pub use memory::MemoryFs;
pub use path::VfsPath;

mod memory;
mod path;

/// A virtual filesystem.
///
/// The filesystem only abstracts file operations. It doesn't abstract directory traversal.
/// While abstracting directory traversal might be desired, it would prevent us from using many popular
/// crates that don't support virtual filesystems (e.g. `walkdir`).
pub trait Vfs {
    /// Returns the metadata for the file at the given path or `Err` if the file doesn't exist or is a directory.
    fn metadata(&self, path: &VfsPath) -> VfsResult<VfsMetadata>;

    /// Returns `true` if the file at the given path exists.
    fn exists(&self, path: &VfsPath) -> bool;

    /// Reads the file at the given path.
    fn read(&self, path: &VfsPath) -> VfsResult<String>;
}

pub type VfsResult<T> = Result<T, VfsError>;

#[derive(Debug)]
pub enum VfsError {
    Io(std::io::Error),
    IsADirectory,
}

impl Display for VfsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VfsError::Io(error) => error.fmt(f),
            VfsError::IsADirectory => f.write_str("is a directory"),
        }
    }
}

impl Error for VfsError {}

impl From<std::io::Error> for VfsError {
    fn from(error: std::io::Error) -> Self {
        VfsError::Io(error)
    }
}

#[derive(Clone, Debug)]
pub struct VfsMetadata {
    revision: FileRevision,
    permission: u32,
}

impl VfsMetadata {
    pub const fn permission(&self) -> u32 {
        self.permission
    }

    pub const fn revision(&self) -> FileRevision {
        self.revision
    }
}

/// A number representing the revision of a file.
///
/// Two revisions that don't compare equal signify that the file has been modified.
/// Revisions aren't guaranteed to be monotonically increasing or in any specific order.
///
/// Possible revisions are:
/// * The last modification time of the file.
/// * The hash of the file's content.
/// * The revision as it comes from an external system, for example the LSP.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FileRevision(u128);

impl FileRevision {
    pub fn new(value: u128) -> Self {
        Self(value)
    }

    pub const fn zero() -> Self {
        Self(0)
    }

    #[must_use]
    pub fn as_u128(self) -> u128 {
        self.0
    }
}

impl From<u128> for FileRevision {
    fn from(value: u128) -> Self {
        FileRevision(value)
    }
}

impl From<u64> for FileRevision {
    fn from(value: u64) -> Self {
        FileRevision(u128::from(value))
    }
}

impl From<FileTime> for FileRevision {
    fn from(value: FileTime) -> Self {
        let seconds = value.seconds() as u128;
        let seconds = seconds << 64;
        let nanos = value.nanoseconds() as u128;

        FileRevision(seconds | nanos)
    }
}

/// Filesystem that reads files from the OS filesystem.
pub struct OsFs;

impl OsFs {
    pub fn snapshot(&self) -> Self {
        Self
    }
}

impl Vfs for OsFs {
    fn metadata(&self, path: &VfsPath) -> VfsResult<VfsMetadata> {
        match path {
            VfsPath::Fs(path) => {
                let metadata = std::fs::metadata(path)?;

                if metadata.is_dir() {
                    return Err(VfsError::IsADirectory);
                }

                let last_modified = FileTime::from_last_modification_time(&metadata);
                let permission = if cfg!(unix) {
                    use std::os::unix::fs::PermissionsExt;

                    metadata.permissions().mode()
                } else {
                    0
                };

                Ok(VfsMetadata {
                    revision: FileRevision::from(last_modified),
                    permission,
                })
            }
            VfsPath::Vendored(_) => todo!(),
        }
    }

    fn exists(&self, path: &VfsPath) -> bool {
        match path {
            VfsPath::Fs(path) => path.is_file(),
            VfsPath::Vendored(_) => todo!(),
        }
    }

    fn read(&self, path: &VfsPath) -> VfsResult<String> {
        match path {
            VfsPath::Fs(path) => Ok(std::fs::read_to_string(path)?),
            VfsPath::Vendored(_) => todo!(),
        }
    }
}

mod tests {}
