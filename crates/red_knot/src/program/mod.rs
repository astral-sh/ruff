use std::collections::hash_map::Entry;
use std::path::PathBuf;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use salsa::Database;

use crate::db::{Db, Jar};
use red_knot_module_resolver::{Db as ResolverDb, Jar as ResolverJar};
use red_knot_python_semantic::{Db as SemanticDb, Jar as SemanticJar};
use ruff_db::file_system::FileSystem;
use ruff_db::vfs::{Vfs, VfsFile};
use ruff_db::{Db as SourceDb, Jar as SourceJar, Upcast};

use crate::Workspace;

pub mod check;

#[salsa::db(SourceJar, ResolverJar, SemanticJar, Jar)]
pub struct Program {
    storage: salsa::Storage<Program>,
    vfs: Vfs,
    fs: Arc<dyn FileSystem + Send + Sync>,
    workspace: Workspace,
}

impl Program {
    pub fn new<Fs>(workspace: Workspace, file_system: Fs) -> Self
    where
        Fs: FileSystem + 'static + Send + Sync,
    {
        Self {
            storage: salsa::Storage::default(),
            vfs: Vfs::default(),
            fs: Arc::new(file_system),
            workspace,
        }
    }

    // pub fn apply_changes<I>(&mut self, changes: I)
    // where
    //     I: IntoIterator<Item = FileWatcherChange>,
    // {
    //     let mut aggregated_changes = AggregatedChanges::default();
    //
    //     aggregated_changes.extend(changes.into_iter().map(|change| FileChange {
    //         id: self.files.intern(&change.path),
    //         kind: change.kind,
    //     }));
    //
    //     let (source, semantic, lint) = self.jars_mut();
    //     for change in aggregated_changes.iter() {
    //         semantic.module_resolver.remove_module_by_file(change.id);
    //         semantic.semantic_indices.remove(&change.id);
    //         source.sources.remove(&change.id);
    //         source.parsed.remove(&change.id);
    //         // TODO: remove all dependent modules as well
    //         semantic.type_store.remove_module(change.id);
    //         lint.lint_syntax.remove(&change.id);
    //         lint.lint_semantic.remove(&change.id);
    //     }
    // }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }
}

impl Upcast<dyn SemanticDb> for Program {
    fn upcast(&self) -> &(dyn SemanticDb + 'static) {
        self
    }
}

impl Upcast<dyn SourceDb> for Program {
    fn upcast(&self) -> &(dyn SourceDb + 'static) {
        self
    }
}

impl Upcast<dyn ResolverDb> for Program {
    fn upcast(&self) -> &(dyn ResolverDb + 'static) {
        self
    }
}

impl ResolverDb for Program {}

impl SemanticDb for Program {}

impl SourceDb for Program {
    fn file_system(&self) -> &dyn FileSystem {
        &*self.fs
    }

    fn vfs(&self) -> &Vfs {
        &self.vfs
    }
}

impl Database for Program {}

impl Db for Program {}

impl salsa::ParallelDatabase for Program {
    fn snapshot(&self) -> salsa::Snapshot<Self> {
        salsa::Snapshot::new(Self {
            storage: self.storage.snapshot(),
            vfs: self.vfs.snapshot(),
            fs: self.fs.clone(),
            workspace: self.workspace.clone(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct FileWatcherChange {
    path: PathBuf,
    kind: FileChangeKind,
}

impl FileWatcherChange {
    pub fn new(path: PathBuf, kind: FileChangeKind) -> Self {
        Self { path, kind }
    }
}

#[derive(Copy, Clone, Debug)]
struct FileChange {
    id: VfsFile,
    kind: FileChangeKind,
}

impl FileChange {
    fn file_id(self) -> VfsFile {
        self.id
    }

    fn kind(self) -> FileChangeKind {
        self.kind
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
}

#[derive(Default, Debug)]
struct AggregatedChanges {
    changes: FxHashMap<VfsFile, FileChangeKind>,
}

impl AggregatedChanges {
    fn add(&mut self, change: FileChange) {
        match self.changes.entry(change.file_id()) {
            Entry::Occupied(mut entry) => {
                let merged = entry.get_mut();

                match (merged, change.kind()) {
                    (FileChangeKind::Created, FileChangeKind::Deleted) => {
                        // Deletion after creations means that ruff never saw the file.
                        entry.remove();
                    }
                    (FileChangeKind::Created, FileChangeKind::Modified) => {
                        // No-op, for ruff, modifying a file that it doesn't yet know that it exists is still considered a creation.
                    }

                    (FileChangeKind::Modified, FileChangeKind::Created) => {
                        // Uhh, that should probably not happen. Continue considering it a modification.
                    }

                    (FileChangeKind::Modified, FileChangeKind::Deleted) => {
                        *entry.get_mut() = FileChangeKind::Deleted;
                    }

                    (FileChangeKind::Deleted, FileChangeKind::Created) => {
                        *entry.get_mut() = FileChangeKind::Modified;
                    }

                    (FileChangeKind::Deleted, FileChangeKind::Modified) => {
                        // That's weird, but let's consider it a modification.
                        *entry.get_mut() = FileChangeKind::Modified;
                    }

                    (FileChangeKind::Created, FileChangeKind::Created)
                    | (FileChangeKind::Modified, FileChangeKind::Modified)
                    | (FileChangeKind::Deleted, FileChangeKind::Deleted) => {
                        // No-op transitions. Some of them should be impossible but we handle them anyway.
                    }
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(change.kind());
            }
        }
    }

    fn extend<I>(&mut self, changes: I)
    where
        I: IntoIterator<Item = FileChange>,
    {
        let iter = changes.into_iter();
        let (lower, _) = iter.size_hint();
        self.changes.reserve(lower);

        for change in iter {
            self.add(change);
        }
    }

    fn iter(&self) -> impl Iterator<Item = FileChange> + '_ {
        self.changes.iter().map(|(id, kind)| FileChange {
            id: *id,
            kind: *kind,
        })
    }
}
