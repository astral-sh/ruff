use std::panic::{AssertUnwindSafe, RefUnwindSafe};
use std::sync::Arc;

use salsa::{Cancelled, Database};

use red_knot_module_resolver::{vendored_typeshed_stubs, Db as ResolverDb, Jar as ResolverJar};
use red_knot_python_semantic::{Db as SemanticDb, Jar as SemanticJar};
use ruff_db::files::{File, Files};
use ruff_db::system::{System, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Jar as SourceJar, Upcast};

use crate::db::{Db, Jar};
use crate::Workspace;

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
            system: Arc::new(system),
            workspace,
        }
    }

    pub fn apply_changes<I>(&mut self, changes: I)
    where
        I: IntoIterator<Item = FileWatcherChange>,
    {
        for change in changes {
            File::touch_path(self, &change.path);
        }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }

    fn with_db<F, T>(&self, f: F) -> Result<T, Cancelled>
    where
        F: FnOnce(&Program) -> T + std::panic::UnwindSafe,
    {
        // The `AssertUnwindSafe` here looks scary, but is a consequence of Salsa's design.
        // Salsa uses panics to implement cancellation and to recover from cycles. However, the Salsa
        // storage isn't `UnwindSafe` or `RefUnwindSafe` because its dependencies `DashMap` and `parking_lot::*` aren't
        // unwind safe.
        //
        // Having to use `AssertUnwindSafe` isn't as big as a deal as it might seem because
        // the `UnwindSafe` and `RefUnwindSafe` traits are designed to catch logical bugs.
        // They don't protect against [UB](https://internals.rust-lang.org/t/pre-rfc-deprecating-unwindsafe/15974).
        // On top of that, `Cancelled` only catches specific Salsa-panics and propagates all other panics.
        //
        // That still leaves us with possible logical bugs in two sources:
        // * In Salsa itself: This must be considered a bug in Salsa and needs fixing upstream.
        //   Reviewing Salsa code specifically around unwind safety seems doable.
        // * Our code: This is the main concern. Luckily, it only involves code that uses internal mutability
        //     and calls into Salsa queries when mutating the internal state. Using `AssertUnwindSafe`
        //     certainly makes it harder to catch these issues in our user code.
        //
        // For now, this is the only solution at hand unless Salsa decides to change its design.
        // [Zulip support thread](https://salsa.zulipchat.com/#narrow/stream/145099-general/topic/How.20to.20use.20.60Cancelled.3A.3Acatch.60)
        let db = &AssertUnwindSafe(self);
        Cancelled::catch(|| f(db))
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
    fn vendored(&self) -> &VendoredFileSystem {
        vendored_typeshed_stubs()
    }

    fn system(&self) -> &dyn System {
        &*self.system
    }

    fn files(&self) -> &Files {
        &self.files
    }
}

impl Database for Program {}

impl Db for Program {}

impl salsa::ParallelDatabase for Program {
    fn snapshot(&self) -> salsa::Snapshot<Self> {
        salsa::Snapshot::new(Self {
            storage: self.storage.snapshot(),
            files: self.files.snapshot(),
            system: self.system.clone(),
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
