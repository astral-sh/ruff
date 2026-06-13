use std::collections::BTreeSet;

use compact_str::CompactString;
use salsa::{Durability, Setter};

use crate::file_revision::FileRevision;
use crate::system::{FileType, SystemPath, SystemPathBuf};
use crate::{Db, FxDashMap};

/// A system directory whose direct children are tracked by Salsa.
#[salsa::input(debug, heap_size=ruff_memory_usage::heap_size)]
pub(super) struct Directory {
    /// The path of the directory (immutable).
    #[returns(deref)]
    path: Box<SystemPath>,

    /// Changes whenever an entry is added, removed, or changes type.
    revision: FileRevision,
}

impl get_size2::GetSize for Directory {}

/// A cached snapshot of the direct children in a directory.
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

#[inline]
pub fn directory_listing<'db>(
    db: &'db dyn Db,
    path: &SystemPath,
) -> Result<&'db DirectoryListing, &'db DirectoryListingError> {
    directory_listing_query(db, db.files().directory(db, path))
}

#[salsa::tracked(returns(as_ref), heap_size=ruff_memory_usage::heap_size)]
fn directory_listing_query(
    db: &dyn Db,
    directory: Directory,
) -> Result<DirectoryListing, DirectoryListingError> {
    directory.revision(db);

    let mut entries = db
        .system()
        .read_directory(directory.path(db))?
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

#[derive(Default)]
pub(super) struct Directories {
    by_path: FxDashMap<SystemPathBuf, Directory>,
}

impl Directories {
    pub(super) fn get_or_create(&self, db: &dyn Db, path: SystemPathBuf) -> Directory {
        if let Some(directory) = self.by_path.get(&path) {
            return *directory;
        }

        *self.by_path.entry(path.clone()).or_insert_with(|| {
            let durability = db
                .files()
                .root(db, &path)
                .map_or(Durability::default(), |root| root.durability(db));
            let durability = Durability::MEDIUM.max(durability);

            Directory::builder(Box::from(path), FileRevision::now())
                .durability(durability)
                .path_durability(Durability::HIGH)
                .new(db)
        })
    }

    pub(super) fn touch(&self, db: &mut dyn Db, path: &SystemPath) {
        if let Some(directory) = self.by_path.get(path).map(|directory| *directory) {
            directory.set_revision(db).to(FileRevision::now());
        }
    }

    pub(super) fn touch_recursive(&self, db: &mut dyn Db, paths: &BTreeSet<SystemPathBuf>) {
        let parents = paths
            .iter()
            .filter_map(|path| path.parent().map(SystemPath::to_path_buf))
            .collect::<BTreeSet<_>>();
        let directories = self
            .by_path
            .iter()
            .filter(|entry| {
                let path = entry.key();
                parents.contains(path) || paths.iter().any(|candidate| path.starts_with(candidate))
            })
            .map(|entry| *entry.value())
            .collect::<Vec<_>>();

        for directory in directories {
            directory.set_revision(db).to(FileRevision::now());
        }
    }

    pub(super) fn touch_all(&self, db: &mut dyn Db) {
        let directories = self
            .by_path
            .iter()
            .map(|entry| *entry.value())
            .collect::<Vec<_>>();

        for directory in directories {
            directory.set_revision(db).to(FileRevision::now());
        }
    }

    pub(super) fn len(&self) -> usize {
        self.by_path.len()
    }
}

#[cfg(test)]
mod tests {
    use crate::Db as _;
    use crate::files::{File, directory_listing, system_path_to_file};
    use crate::system::{DbWithWritableSystem as _, SystemPath, WritableSystem as _};
    use crate::tests::TestDb;

    #[test]
    fn listing_is_sorted_and_cached() -> std::io::Result<()> {
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

        db.writable_system()
            .write_file(SystemPath::new("src/new.py"), "")?;
        assert_eq!(
            directory_listing(&db, path).unwrap().file_type("new.py"),
            None
        );

        File::sync_path(&mut db, SystemPath::new("src/new.py"));
        assert!(
            directory_listing(&db, path)
                .unwrap()
                .file_type("new.py")
                .is_some()
        );

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

    #[test]
    fn content_change_does_not_invalidate_directory() -> std::io::Result<()> {
        let mut db = TestDb::new();
        db.write_file("src/a.py", "old")?;
        system_path_to_file(&db, SystemPath::new("src/a.py")).unwrap();

        let directory = db.files().directory(&db, SystemPath::new("src"));
        directory_listing(&db, SystemPath::new("src")).unwrap();
        let revision = directory.revision(&db);

        db.writable_system()
            .write_file(SystemPath::new("src/a.py"), "new")?;
        File::sync_path(&mut db, SystemPath::new("src/a.py"));

        assert_eq!(directory.revision(&db), revision);
        Ok(())
    }

    #[test]
    fn structural_changes_invalidate_directory() -> std::io::Result<()> {
        let mut db = TestDb::new();
        db.write_file("src/existing.py", "")?;
        let existing = system_path_to_file(&db, SystemPath::new("src/existing.py")).unwrap();

        let directory = SystemPath::new("src");
        assert!(
            directory_listing(&db, directory)
                .unwrap()
                .file_type("existing.py")
                .is_some()
        );

        db.writable_system()
            .memory_file_system()
            .remove_file("src/existing.py")?;
        existing.sync(&mut db);
        assert_eq!(
            directory_listing(&db, directory)
                .unwrap()
                .file_type("existing.py"),
            None
        );

        db.writable_system()
            .write_file(SystemPath::new("src/new.py"), "")?;
        File::sync_path(&mut db, SystemPath::new("src/new.py"));
        assert!(
            directory_listing(&db, directory)
                .unwrap()
                .file_type("new.py")
                .is_some()
        );

        Ok(())
    }

    #[test]
    fn syncing_directory_invalidates_its_listing() -> std::io::Result<()> {
        let mut db = TestDb::new();
        db.write_file("src/a.py", "")?;

        let directory = SystemPath::new("src");
        directory_listing(&db, directory).unwrap();

        db.writable_system()
            .write_file(SystemPath::new("src/new.py"), "")?;
        File::sync_path(&mut db, SystemPath::new("src"));

        assert!(
            directory_listing(&db, directory)
                .unwrap()
                .file_type("new.py")
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn recursive_sync_invalidates_parent_listing() -> std::io::Result<()> {
        let mut db = TestDb::new();
        db.writable_system()
            .memory_file_system()
            .create_directory_all("src/package")?;

        let src = SystemPath::new("src");
        assert!(
            directory_listing(&db, src)
                .unwrap()
                .file_type("package")
                .is_some()
        );

        db.writable_system()
            .memory_file_system()
            .remove_directory("src/package")?;
        crate::files::Files::sync_all_recursive(&mut db, [SystemPath::new("src/package")]);

        assert_eq!(
            directory_listing(&db, src).unwrap().file_type("package"),
            None
        );
        Ok(())
    }
}
