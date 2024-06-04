use std::path::PathBuf;
use std::sync::Arc;

use countme::Count;
use dashmap::mapref::entry::Entry;
use filetime::FileTime;

use ruff_python_ast::{ModModule, PySourceType};
use ruff_python_parser::Parsed;

use crate::FxDashMap;

/// A file can be considered unchanged for as long as it has the same revision.
///
/// The value of the revision itself has no meaning other than to encode the version of the file.
/// Therefore, it's not possible to identify which revision is newer by comparing the values.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FileRevision {
    LastModified(FileTime),
    #[allow(unused)]
    ContentHash(u128),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum FileStatus {
    Exists,
    /// The file was deleted, didn't exist to begin with or the path isn't a file.
    Deleted,
}

#[salsa::input(jar=Jar)]
pub struct File {
    #[return_ref]
    pub path: PathBuf,

    pub permissions: u32,

    pub revision: FileRevision,

    pub status: FileStatus,

    #[allow(unused)]
    count: Count<File>,
}

impl File {
    /// Updates the metadata of the file.
    #[tracing::instrument(level = "debug", skip(db))]
    pub fn touch(self, db: &mut dyn Db) {
        let path = self.path(db);

        if let Ok(metadata) = path.metadata() {
            let last_modified = FileTime::from_last_modification_time(&metadata);
            #[cfg(unix)]
            let permissions = if cfg!(unix) {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode()
            } else {
                0
            };

            self.set_revision(db)
                .to(FileRevision::LastModified(last_modified));
            self.set_permissions(db).to(permissions);
            self.set_status(db).to(FileStatus::Exists);
        } else {
            self.delete(db);
        }
    }

    pub fn delete(self, db: &mut dyn Db) {
        self.set_status(db).to(FileStatus::Deleted);
        self.set_permissions(db).to(0);
        self.set_revision(db)
            .to(FileRevision::LastModified(FileTime::zero()));
    }

    pub fn exists(self, db: &dyn Db) -> bool {
        self.status(db) == FileStatus::Exists
    }
}

#[salsa::tracked(jar=Jar)]
impl File {
    #[salsa::tracked]
    pub fn source(self, db: &dyn Db) -> SourceText {
        // Read the revision to force a re-run of this query when the file gets updated.
        let _ = self.revision(db);
        let text = std::fs::read_to_string(self.path(db)).unwrap_or_default();

        SourceText {
            text: Arc::from(text),
            count: Count::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Files {
    inner: Arc<FilesInner>,
}

impl Files {
    pub(super) fn resolve(&self, db: &dyn Db, path: PathBuf) -> File {
        match self.inner.by_path.entry(path.clone()) {
            Entry::Occupied(entry) => {
                let file = entry.get();
                *file
            }
            Entry::Vacant(entry) => {
                let metadata = path.metadata();

                let file = if let Ok(metadata) = metadata {
                    let (last_modified, permissions, file_status) = if metadata.is_file() {
                        let last_modified = FileTime::from_last_modification_time(&metadata);
                        #[cfg(unix)]
                        let permissions = if cfg!(unix) {
                            use std::os::unix::fs::PermissionsExt;
                            metadata.permissions().mode()
                        } else {
                            0
                        };

                        (last_modified, permissions, FileStatus::Exists)
                    } else {
                        (FileTime::zero(), 0, FileStatus::Deleted)
                    };

                    File::new(
                        db,
                        path,
                        permissions,
                        FileRevision::LastModified(last_modified),
                        file_status,
                        Count::default(),
                    )
                } else {
                    File::new(
                        db,
                        path,
                        0,
                        FileRevision::LastModified(FileTime::zero()),
                        FileStatus::Deleted,
                        Count::default(),
                    )
                };

                // TODO: Set a higher durability for std files.

                entry.insert(file);

                file
            }
        }
    }
}

#[derive(Debug, Default)]
struct FilesInner {
    by_path: FxDashMap<PathBuf, File>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SourceText {
    pub text: Arc<str>,
    count: Count<SourceText>,
}

impl SourceText {
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar, no_eq)]
pub fn parse(db: &dyn Db, file: File) -> Arc<Parsed<ModModule>> {
    let source = file.source(db);

    let result = ruff_python_parser::parse_unchecked_source(source.text(), PySourceType::Python);

    Arc::new(result)
}

#[salsa::jar(db=Db)]
pub struct Jar(File, File_source, parse);

pub trait Db: salsa::DbWithJar<Jar> {
    fn file(&self, path: PathBuf) -> File;
}
