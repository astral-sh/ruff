use std::fmt::Formatter;

use path_slash::PathExt;
use salsa::Durability;

use crate::file_revision::FileRevision;
use crate::system::{SystemPath, SystemPathBuf};
use crate::Db;

/// A root path for files tracked by the database.
///
/// We currently create roots for:
/// * static module resolution paths
/// * the workspace root
///
/// The main usage of file roots is to determine a file's durability. But it can also be used
/// to make a salsa query dependent on whether a file in a root has changed without writing any
/// manual invalidation logic.
#[salsa::input]
pub struct FileRoot {
    /// The path of a root is guaranteed to never change.
    #[return_ref]
    path_buf: SystemPathBuf,

    /// The kind of the root at the time of its creation.
    kind_at_time_of_creation: FileRootKind,

    /// A revision that changes when the contents of the source root change.
    ///
    /// The revision changes when a new file was added, removed, or changed inside this source root.
    pub revision: FileRevision,
}

impl FileRoot {
    pub fn path(self, db: &dyn Db) -> &SystemPath {
        self.path_buf(db)
    }

    pub fn durability(self, db: &dyn Db) -> salsa::Durability {
        self.kind_at_time_of_creation(db).durability()
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FileRootKind {
    /// The root of a workspace.
    Workspace,

    /// A non-workspace module resolution search path.
    LibrarySearchPath,
}

impl FileRootKind {
    const fn durability(self) -> Durability {
        match self {
            FileRootKind::Workspace => Durability::LOW,
            FileRootKind::LibrarySearchPath => Durability::HIGH,
        }
    }
}

#[derive(Default)]
pub(super) struct FileRoots {
    by_path: matchit::Router<FileRoot>,
    roots: Vec<FileRoot>,
}

impl FileRoots {
    /// Tries to add a new root for `path` and returns the root.
    ///
    /// The root isn't added nor is the file root's kind updated if a root for `path` already exists.
    pub(super) fn try_add(
        &mut self,
        db: &dyn Db,
        path: SystemPathBuf,
        kind: FileRootKind,
    ) -> FileRoot {
        // SAFETY: Guaranteed to succeed because `path` is a UTF-8 that only contains Unicode characters.
        let normalized_path = path.as_std_path().to_slash().unwrap();

        if let Ok(existing) = self.by_path.at(&normalized_path) {
            // Only if it is an exact match
            if existing.value.path(db) == &*path {
                return *existing.value;
            }
        }

        // normalize the path to use `/` separators and escape the '{' and '}' characters,
        // which matchit uses for routing parameters
        let mut route = normalized_path.replace('{', "{{").replace('}', "}}");

        // Insert a new source root
        let root = FileRoot::builder(path, kind, FileRevision::now())
            .durability(Durability::HIGH)
            .revision_durability(kind.durability())
            .new(db);

        // Insert a path that matches the root itself
        self.by_path.insert(route.clone(), root).unwrap();

        // Insert a path that matches all subdirectories and files
        route.push_str("/{*filepath}");

        self.by_path.insert(route, root).unwrap();
        self.roots.push(root);

        root
    }

    /// Returns the closest root for `path` or `None` if no root contains `path`.
    pub(super) fn at(&self, path: &SystemPath) -> Option<FileRoot> {
        // SAFETY: Guaranteed to succeed because `path` is a UTF-8 that only contains Unicode characters.
        let normalized_path = path.as_std_path().to_slash().unwrap();
        let entry = self.by_path.at(&normalized_path).ok()?;
        Some(*entry.value)
    }

    pub(super) fn all(&self) -> impl Iterator<Item = FileRoot> + '_ {
        self.roots.iter().copied()
    }
}

impl std::fmt::Debug for FileRoots {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FileRoots").field(&self.roots).finish()
    }
}

impl PartialEq for FileRoots {
    fn eq(&self, other: &Self) -> bool {
        self.roots.eq(&other.roots)
    }
}
