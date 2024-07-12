use std::panic::{AssertUnwindSafe, RefUnwindSafe};
use std::sync::Arc;

use salsa::{Cancelled, Database};

use red_knot_module_resolver::{vendored_typeshed_stubs, Db as ResolverDb, Jar as ResolverJar};
use red_knot_python_semantic::{Db as SemanticDb, Jar as SemanticJar};
use ruff_db::files::{system_path_to_file, File, Files};
use ruff_db::system::{System, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Jar as SourceJar, Upcast};

use crate::lint::{lint_semantic, lint_syntax, unwind_if_cancelled};
use crate::workspace::{Project, Workspace};

pub trait Db: salsa::DbWithJar<Jar> + SemanticDb + Upcast<dyn SemanticDb> {}

#[salsa::jar(db=Db)]
pub struct Jar(
    Workspace,
    Project,
    lint_syntax,
    lint_semantic,
    unwind_if_cancelled,
);

#[salsa::db(SourceJar, ResolverJar, SemanticJar, Jar)]
pub struct RootDatabase {
    storage: salsa::Storage<RootDatabase>,
    files: Files,
    system: Arc<dyn System + Send + Sync + RefUnwindSafe>,
}

impl RootDatabase {
    pub fn new<S>(system: S) -> Self
    where
        S: System + Send + Sync + RefUnwindSafe + 'static,
    {
        Self {
            storage: salsa::Storage::default(),
            files: Files::default(),
            system: Arc::new(system),
        }
    }

    pub fn apply_changes<I>(&mut self, changes: I)
    where
        I: IntoIterator<Item = FileWatcherChange>,
    {
        let workspace = Workspace::get(self);
        let workspace_path = workspace.path(self).to_path_buf();

        let mut structural_change = false;
        for change in changes {
            if change.path.ends_with(".gitignore") || change.path.ends_with("pyproject.toml") {
                if change.path.starts_with(&workspace_path) {
                    structural_change = true;
                }
            }

            // Reload the project when a new file was added. This is necessary because the file might be excluded
            // by a gitignore. This also handles that the file automatically gets added to the `open_files` list.
            match change.kind {
                FileChangeKind::Created => {
                    if workspace.project(self, &change.path).is_some() {
                        structural_change = true;
                    }
                }
                FileChangeKind::Modified => {}
                FileChangeKind::Deleted => {
                    if let Some(project) = workspace.project(self, &change.path) {
                        if let Some(file) = system_path_to_file(self, &change.path) {
                            project.close_file(self, file);
                        }
                    }
                }
            }

            File::touch_path(self, &change.path);
        }

        if structural_change {
            workspace.reload(self).unwrap();
        }
    }

    pub(crate) fn with_db<F, T>(&self, f: F) -> Result<T, Cancelled>
    where
        F: FnOnce(&RootDatabase) -> T + std::panic::UnwindSafe,
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

impl Upcast<dyn SemanticDb> for RootDatabase {
    fn upcast(&self) -> &(dyn SemanticDb + 'static) {
        self
    }
}

impl Upcast<dyn SourceDb> for RootDatabase {
    fn upcast(&self) -> &(dyn SourceDb + 'static) {
        self
    }
}

impl Upcast<dyn ResolverDb> for RootDatabase {
    fn upcast(&self) -> &(dyn ResolverDb + 'static) {
        self
    }
}

impl ResolverDb for RootDatabase {}

impl SemanticDb for RootDatabase {}

impl SourceDb for RootDatabase {
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

impl Database for RootDatabase {}

impl Db for RootDatabase {}

impl salsa::ParallelDatabase for RootDatabase {
    fn snapshot(&self) -> salsa::Snapshot<Self> {
        salsa::Snapshot::new(Self {
            storage: self.storage.snapshot(),
            files: self.files.snapshot(),
            system: Arc::clone(&self.system),
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
