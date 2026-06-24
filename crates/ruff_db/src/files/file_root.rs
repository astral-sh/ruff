use path_slash::PathExt;
use salsa::Durability;

use crate::Db;
use crate::system::{SystemPath, SystemPathBuf};

/// A root path for files tracked by the database.
///
/// We currently create roots for:
/// * static module resolution paths
/// * the project root
///
/// File roots determine the durability of files and directories.
#[salsa::input(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct FileRoot {
    /// The path of a root is guaranteed to never change.
    #[returns(deref)]
    pub path: Box<SystemPath>,

    /// The kind of the root at the time of its creation.
    pub kind_at_time_of_creation: FileRootKind,
}

impl FileRoot {
    pub fn durability(self, db: &dyn Db) -> salsa::Durability {
        self.kind_at_time_of_creation(db).durability()
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub enum FileRootKind {
    /// The root of a project.
    Project,

    /// A non-project module resolution search path.
    SearchPath,
}

impl FileRootKind {
    const fn durability(self) -> Durability {
        match self {
            FileRootKind::Project => Durability::LOW,
            FileRootKind::SearchPath => Durability::HIGH,
        }
    }
}

#[derive(Default)]
pub(super) struct FileRoots {
    by_path: matchit::Router<FileRoot>,
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

        tracing::debug!("Adding new file root '{path}' of kind {kind:?}");

        // normalize the path to use `/` separators and escape the '{' and '}' characters,
        // which matchit uses for routing parameters
        let mut route = normalized_path.replace('{', "{{").replace('}', "}}");

        // Insert a new source root
        let root = FileRoot::builder(path.into(), kind)
            .durability(Durability::HIGH)
            .new(db);

        // Insert a path that matches the root itself
        self.by_path.insert(route.clone(), root).unwrap();

        // Insert a path that matches all subdirectories and files
        if !route.ends_with("/") {
            route.push('/');
        }
        route.push_str("{*filepath}");

        self.by_path.insert(route, root).unwrap();

        root
    }

    /// Returns the closest root for `path` or `None` if no root contains `path`.
    pub(super) fn at(&self, path: &SystemPath) -> Option<FileRoot> {
        // SAFETY: Guaranteed to succeed because `path` is a UTF-8 that only contains Unicode characters.
        let normalized_path = path.as_std_path().to_slash().unwrap();
        let entry = self.by_path.at(&normalized_path).ok()?;
        Some(*entry.value)
    }
}
