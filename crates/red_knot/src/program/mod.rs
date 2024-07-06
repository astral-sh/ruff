use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::Arc;

use salsa::{Cancelled, Database};

use crate::db::{Db, Jar};
use crate::Workspace;
use red_knot_module_resolver::{Db as ResolverDb, Jar as ResolverJar};
use red_knot_python_semantic::{Db as SemanticDb, Jar as SemanticJar};
use ruff_db::files::{File, FilePath, Files};
use ruff_db::system::{System, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Jar as SourceJar, Upcast};

mod check;

#[salsa::db(SourceJar, ResolverJar, SemanticJar, Jar)]
pub struct Program {
    storage: salsa::Storage<Program>,
    files: Files,
    system: Arc<dyn System + Send + Sync + RefUnwindSafe>,
    workspace: Workspace,
}

impl Program {
    pub fn new<S>(workspace: Workspace, system: S) -> Self
    where
        S: System + 'static + Send + Sync + RefUnwindSafe,
    {
        Self {
            storage: salsa::Storage::default(),
            files: Files::default(),
            // TODO correctly initialize vendored file system
            vendored: VendoredFileSystem::default(),
            system: Arc::new(system),
            workspace,
        }
    }

    pub fn apply_changes<I>(&mut self, changes: I)
    where
        I: IntoIterator<Item = FileWatcherChange>,
    {
        for change in changes {
            File::touch_path(self, &FilePath::system(change.path));
        }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }

    #[allow(clippy::unnecessary_wraps)]
    fn with_db<F, T>(&self, f: F) -> Result<T, Cancelled>
    where
        F: FnOnce(&Program) -> T + UnwindSafe,
    {
        // TODO: Catch in `Caancelled::catch`
        //  See https://salsa.zulipchat.com/#narrow/stream/145099-general/topic/How.20to.20use.20.60Cancelled.3A.3Acatch.60
        Ok(f(self))
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
        vendored_typeshed_stubs()
        &self.vendored
    }

    fn system(&self) -> &dyn System {
        &*self.system
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
            files: self.files.snapshot(),
            vendored: self.vendored.snapshot(),
            workspace: self.workspace.clone(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct FileWatcherChange {
    path: SystemPathBuf,
    #[allow(unused)]
    kind: FileChangeKind,
}

impl FileWatcherChange {
    pub fn new(path: SystemPathBuf, kind: FileChangeKind) -> Self {
        Self { path, kind }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
}
