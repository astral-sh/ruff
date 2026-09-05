use compact_str::CompactString;

use super::private::FileStatus;
use super::{File, FilePath};
use crate::Db;
use crate::system::{FileType, SystemPath};

/// A cached snapshot of the direct children in a directory.
///
/// The entries are sorted by name for efficient lookups.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct DirectoryListing(Box<[(CompactString, FileType)]>);

impl DirectoryListing {
    /// Returns the type of the entry named `name`, if present.
    pub fn file_type(&self, name: &str) -> Option<FileType> {
        self.0
            .binary_search_by(|(candidate, _)| candidate.as_str().cmp(name))
            .ok()
            .map(|index| self.0[index].1)
    }

    /// Returns whether any entry name starts with `prefix`.
    pub fn contains_name_with_prefix(&self, prefix: &str) -> bool {
        let index = self
            .0
            .partition_point(|(candidate, _)| candidate.as_str() < prefix);
        self.0
            .get(index)
            .is_some_and(|(name, _)| name.starts_with(prefix))
    }

    /// Returns whether `name` resolves to a file, following symbolic links.
    pub fn entry_is_file(&self, db: &dyn Db, directory: &SystemPath, name: &str) -> bool {
        match self.file_type(name) {
            Some(FileType::File) => true,
            Some(FileType::Directory) | None => false,
            Some(FileType::Symlink) => super::system_path_to_file(db, directory.join(name)).is_ok(),
        }
    }

    /// Returns whether `name` resolves to a directory, following symbolic links.
    pub fn entry_is_directory(&self, db: &dyn Db, directory: &SystemPath, name: &str) -> bool {
        match self.file_type(name) {
            Some(FileType::Directory) => true,
            Some(FileType::File) | None => false,
            Some(FileType::Symlink) => db.system().is_directory(&directory.join(name)),
        }
    }

    /// Iterates over the entries in the directory in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, FileType)> {
        self.0
            .iter()
            .map(|(name, file_type)| (name.as_str(), *file_type))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize, thiserror::Error)]
#[error("{message}")]
pub struct DirectoryListingError {
    #[get_size(ignore)]
    kind: std::io::ErrorKind,
    message: Box<str>,
}

impl From<std::io::Error> for DirectoryListingError {
    fn from(error: std::io::Error) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string().into_boxed_str(),
        }
    }
}

/// Interns a directory system path and returns a salsa `File` ingredient.
///
/// Returns `Err` if the path doesn't exist, isn't accessible, or if the path doesn't point to a directory.
#[inline]
pub fn system_path_to_directory(
    db: &dyn Db,
    path: impl AsRef<SystemPath>,
) -> Result<File, DirectoryListingError> {
    let file = db.files().system(db, path.as_ref());

    match file.status(db) {
        FileStatus::IsADirectory => Ok(file),
        FileStatus::Exists => Err(std::io::Error::from(std::io::ErrorKind::NotADirectory).into()),
        FileStatus::NotFound => Err(std::io::Error::from(std::io::ErrorKind::NotFound).into()),
    }
}

#[inline]
pub fn directory_listing<'db>(
    db: &'db dyn Db,
    path: &SystemPath,
) -> Result<&'db DirectoryListing, DirectoryListingError> {
    let directory = system_path_to_directory(db, path)?;
    directory_listing_query(db, directory).map_err(Clone::clone)
}

#[salsa::tracked(returns(as_ref), heap_size=ruff_memory_usage::heap_size)]
fn directory_listing_query(
    db: &dyn Db,
    directory: File,
) -> Result<DirectoryListing, DirectoryListingError> {
    let _ = directory.revision(db);
    let _ = directory.permissions(db);

    let path = match directory.path(db) {
        FilePath::System(path) => path,
        FilePath::Vendored(_) | FilePath::SystemVirtual(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "directory listings are only supported for system paths",
            )
            .into());
        }
    };

    let mut entries = db
        .system()
        .read_directory(path)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let file_type = entry.file_type();
            let path = entry.into_path();
            let name = path.file_name()?;
            Some((CompactString::from(name), file_type))
        })
        .collect::<Vec<_>>();

    entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    Ok(DirectoryListing(entries.into_boxed_slice()))
}

#[cfg(test)]
mod tests {
    use crate::files::directory_listing;
    use crate::system::{DbWithWritableSystem as _, SystemPath};
    use crate::tests::TestDb;

    #[test]
    fn listing_is_sorted() -> std::io::Result<()> {
        let mut db = TestDb::new();
        db.write_file("src/z.py", "")?;
        db.write_file("src/a.py", "")?;

        let path = SystemPath::new("src");
        let listing = directory_listing(&db, path).unwrap();
        assert_eq!(
            listing.iter().map(|(name, _)| name).collect::<Vec<_>>(),
            ["a.py", "z.py"]
        );
        assert!(listing.contains_name_with_prefix("a"));
        assert!(!listing.contains_name_with_prefix("b"));

        Ok(())
    }

    #[test]
    fn empty_and_unavailable_listing() {
        let db = TestDb::new();

        assert_eq!(
            directory_listing(&db, SystemPath::new("/"))
                .unwrap()
                .iter()
                .next(),
            None
        );

        assert_eq!(
            directory_listing(&db, SystemPath::new("missing"))
                .unwrap_err()
                .kind,
            std::io::ErrorKind::NotFound
        );
    }
}
