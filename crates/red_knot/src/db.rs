use std::panic::{AssertUnwindSafe, RefUnwindSafe};
use std::sync::Arc;

use salsa::{Cancelled, Database, DbWithJar};

use red_knot_module_resolver::{vendored_typeshed_stubs, Db as ResolverDb, Jar as ResolverJar};
use red_knot_python_semantic::{Db as SemanticDb, Jar as SemanticJar};
use ruff_db::files::{system_path_to_file, File, Files};
use ruff_db::program::{Program, ProgramSettings};
use ruff_db::system::System;
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Jar as SourceJar, Upcast};

use crate::lint::{lint_semantic, lint_syntax, unwind_if_cancelled, Diagnostics};
use crate::watch::{FileChangeKind, FileWatcherChange};
use crate::workspace::{check_file, Package, Workspace, WorkspaceMetadata};

pub trait Db: DbWithJar<Jar> + SemanticDb + Upcast<dyn SemanticDb> {}

#[salsa::jar(db=Db)]
pub struct Jar(
    Workspace,
    Package,
    lint_syntax,
    lint_semantic,
    unwind_if_cancelled,
);

#[salsa::db(SourceJar, ResolverJar, SemanticJar, Jar)]
pub struct RootDatabase {
    workspace: Option<Workspace>,
    storage: salsa::Storage<RootDatabase>,
    files: Files,
    system: Arc<dyn System + Send + Sync + RefUnwindSafe>,
}

impl RootDatabase {
    pub fn new<S>(workspace: WorkspaceMetadata, settings: ProgramSettings, system: S) -> Self
    where
        S: System + 'static + Send + Sync + RefUnwindSafe,
    {
        let mut db = Self {
            workspace: None,
            storage: salsa::Storage::default(),
            files: Files::default(),
            system: Arc::new(system),
        };

        let workspace = Workspace::from_metadata(&db, workspace);
        // Initialize the `Program` singleton
        Program::from_settings(&db, settings);

        db.workspace = Some(workspace);
        db
    }

    pub fn workspace(&self) -> Workspace {
        // SAFETY: The workspace is always initialized in `new`.
        self.workspace.unwrap()
    }

    #[tracing::instrument(level = "debug", skip(self, changes))]
    pub fn apply_changes(&mut self, changes: Vec<FileWatcherChange>) {
        let workspace = self.workspace();
        let workspace_path = workspace.root(self).to_path_buf();

        // TODO: Optimize change tracking by only reloading a package if a file that is part of the package was changed.
        let mut structural_change = false;
        for change in changes {
            if matches!(
                change.path.file_name(),
                Some(".gitignore" | ".ignore" | "ruff.toml" | ".ruff.toml" | "pyproject.toml")
            ) {
                // Changes to ignore files or settings can change the workspace structure or add/remove files
                // from packages.
                structural_change = true;
            } else {
                match change.kind {
                    FileChangeKind::Created => {
                        // Reload the package when a new file was added. This is necessary because the file might be excluded
                        // by a gitignore.
                        if workspace.package(self, &change.path).is_some() {
                            structural_change = true;
                        }
                    }
                    FileChangeKind::Modified => {}
                    FileChangeKind::Deleted => {
                        if let Some(package) = workspace.package(self, &change.path) {
                            if let Some(file) = system_path_to_file(self, &change.path) {
                                package.remove_file(self, file);
                            }
                        }
                    }
                }
            }

            File::touch_path(self, &change.path);
        }

        if structural_change {
            match WorkspaceMetadata::from_path(&workspace_path, self.system()) {
                Ok(metadata) => {
                    tracing::debug!("Reload workspace after structural change.");
                    // TODO: Handle changes in the program settings.
                    workspace.reload(self, metadata);
                }
                Err(error) => {
                    tracing::error!("Failed to load workspace, keep old workspace: {error}");
                }
            }
        }
    }

    /// Checks all open files in the workspace and its dependencies.
    pub fn check(&self) -> Result<Vec<String>, Cancelled> {
        self.with_db(|db| db.workspace().check(db))
    }

    pub fn check_file(&self, file: File) -> Result<Diagnostics, Cancelled> {
        self.with_db(|db| check_file(db, file))
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

    fn upcast_mut(&mut self) -> &mut (dyn SemanticDb + 'static) {
        self
    }
}

impl Upcast<dyn SourceDb> for RootDatabase {
    fn upcast(&self) -> &(dyn SourceDb + 'static) {
        self
    }

    fn upcast_mut(&mut self) -> &mut (dyn SourceDb + 'static) {
        self
    }
}

impl Upcast<dyn ResolverDb> for RootDatabase {
    fn upcast(&self) -> &(dyn ResolverDb + 'static) {
        self
    }
    fn upcast_mut(&mut self) -> &mut (dyn ResolverDb + 'static) {
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
            workspace: self.workspace,
            storage: self.storage.snapshot(),
            files: self.files.snapshot(),
            system: self.system.clone(),
        })
    }
}
