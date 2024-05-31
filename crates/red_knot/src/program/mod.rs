use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::db::{
    Database, Db, DbRuntime, DbWithJar, HasJar, HasJars, JarsStorage, LintDb, LintJar,
    ParallelDatabase, QueryResult, SemanticDb, SemanticJar, Snapshot, SourceDb, SourceJar, Upcast,
};
use crate::files::{FileId, Files};
use crate::Workspace;

pub mod check;

#[derive(Debug)]
pub struct Program {
    jars: JarsStorage<Program>,
    files: Files,
    workspace: Workspace,
}

impl Program {
    pub fn new(workspace: Workspace) -> Self {
        Self {
            jars: JarsStorage::default(),
            files: Files::default(),
            workspace,
        }
    }

    pub fn apply_changes<I>(&mut self, changes: I)
    where
        I: IntoIterator<Item = FileWatcherChange>,
    {
        let mut aggregated_changes = AggregatedChanges::default();

        aggregated_changes.extend(changes.into_iter().map(|change| FileChange {
            id: self.files.intern(&change.path),
            kind: change.kind,
        }));

        let (source, semantic, lint) = self.jars_mut();
        for change in aggregated_changes.iter() {
            semantic.module_resolver.remove_module_by_file(change.id);
            semantic.symbol_tables.remove(&change.id);
            source.sources.remove(&change.id);
            source.parsed.remove(&change.id);
            // TODO: remove all dependent modules as well
            semantic.type_store.remove_module(change.id);
            lint.lint_syntax.remove(&change.id);
            lint.lint_semantic.remove(&change.id);
        }
    }

    pub fn files(&self) -> &Files {
        &self.files
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }
}

impl SourceDb for Program {
    fn file_id(&self, path: &Path) -> FileId {
        self.files.intern(path)
    }

    fn file_path(&self, file_id: FileId) -> Arc<Path> {
        self.files.path(file_id)
    }
}

impl DbWithJar<SourceJar> for Program {}

impl SemanticDb for Program {}

impl DbWithJar<SemanticJar> for Program {}

impl LintDb for Program {}

impl DbWithJar<LintJar> for Program {}

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

impl Upcast<dyn LintDb> for Program {
    fn upcast(&self) -> &(dyn LintDb + 'static) {
        self
    }
}

impl Db for Program {}

impl Database for Program {
    fn runtime(&self) -> &DbRuntime {
        self.jars.runtime()
    }

    fn runtime_mut(&mut self) -> &mut DbRuntime {
        self.jars.runtime_mut()
    }
}

impl ParallelDatabase for Program {
    fn snapshot(&self) -> Snapshot<Self> {
        Snapshot::new(Self {
            jars: self.jars.snapshot(),
            files: self.files.snapshot(),
            workspace: self.workspace.clone(),
        })
    }
}

impl HasJars for Program {
    type Jars = (SourceJar, SemanticJar, LintJar);

    fn jars(&self) -> QueryResult<&Self::Jars> {
        self.jars.jars()
    }

    fn jars_mut(&mut self) -> &mut Self::Jars {
        self.jars.jars_mut()
    }
}

impl HasJar<SourceJar> for Program {
    fn jar(&self) -> QueryResult<&SourceJar> {
        Ok(&self.jars()?.0)
    }

    fn jar_mut(&mut self) -> &mut SourceJar {
        &mut self.jars_mut().0
    }
}

impl HasJar<SemanticJar> for Program {
    fn jar(&self) -> QueryResult<&SemanticJar> {
        Ok(&self.jars()?.1)
    }

    fn jar_mut(&mut self) -> &mut SemanticJar {
        &mut self.jars_mut().1
    }
}

impl HasJar<LintJar> for Program {
    fn jar(&self) -> QueryResult<&LintJar> {
        Ok(&self.jars()?.2)
    }

    fn jar_mut(&mut self) -> &mut LintJar {
        &mut self.jars_mut().2
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
    id: FileId,
    kind: FileChangeKind,
}

impl FileChange {
    fn file_id(self) -> FileId {
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
    changes: FxHashMap<FileId, FileChangeKind>,
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
