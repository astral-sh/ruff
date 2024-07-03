use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::Arc;

use salsa::Database;

use red_knot_module_resolver::{Db as ResolverDb, Jar as ResolverJar};
use red_knot_python_semantic::{Db as SemanticDb, Jar as SemanticJar};
use ruff_db::file_system::{FileSystem, FileSystemPathBuf};
use ruff_db::vfs::{Vfs, VfsFile, VfsPath};
use ruff_db::{Db as SourceDb, Jar as SourceJar, Upcast};

use crate::db::{Db, Jar};
use crate::Workspace;

mod check;

#[salsa::db(SourceJar, ResolverJar, SemanticJar, Jar)]
pub struct Program {
    storage: salsa::Storage<Program>,
    vfs: Vfs,
    fs: Arc<dyn FileSystem + Send + Sync + RefUnwindSafe>,
    workspace: Workspace,
}

impl Program {
    pub fn new<Fs>(workspace: Workspace, file_system: Fs) -> Self
    where
        Fs: FileSystem + 'static + Send + Sync + RefUnwindSafe,
    {
        Self {
            storage: salsa::Storage::default(),
            vfs: Vfs::default(),
            fs: Arc::new(file_system),
            workspace,
        }
    }

    pub fn apply_changes<I>(&mut self, changes: I)
    where
        I: IntoIterator<Item = FileWatcherChange>,
    {
        for change in changes {
            VfsFile::touch_path(self, &VfsPath::file_system(change.path));
        }
    }

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
    path: FileSystemPathBuf,
    #[allow(unused)]
    kind: FileChangeKind,
}

impl FileWatcherChange {
    pub fn new(path: FileSystemPathBuf, kind: FileChangeKind) -> Self {
        Self { path, kind }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
}
