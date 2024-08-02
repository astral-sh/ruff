use std::panic::{AssertUnwindSafe, RefUnwindSafe};

use red_knot_module_resolver::{vendored_typeshed_stubs, Db as ResolverDb};
use red_knot_python_semantic::Db as SemanticDb;
use ruff_db::files::{File, Files};
use ruff_db::program::{Program, ProgramSettings};
use ruff_db::system::System;
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Upcast};
use salsa::Cancelled;

use crate::lint::Diagnostics;
use crate::workspace::{check_file, Workspace, WorkspaceMetadata};

mod changes;

#[salsa::db]
pub trait Db: SemanticDb + Upcast<dyn SemanticDb> {}

#[salsa::db]
pub struct RootDatabase {
    workspace: Option<Workspace>,
    storage: salsa::Storage<RootDatabase>,
    files: Files,
    system: Box<dyn System + Send + Sync + RefUnwindSafe>,
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
            system: Box::new(system),
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

#[salsa::db]
impl ResolverDb for RootDatabase {}

#[salsa::db]
impl SemanticDb for RootDatabase {}

#[salsa::db]
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

#[salsa::db]
impl salsa::Database for RootDatabase {}

#[salsa::db]
impl Db for RootDatabase {}

#[cfg(test)]
pub(crate) mod tests {
    use crate::db::Db;
    use red_knot_module_resolver::{vendored_typeshed_stubs, Db as ResolverDb};
    use red_knot_python_semantic::Db as SemanticDb;
    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem, System, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_db::{Db as SourceDb, Upcast};

    #[salsa::db]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                system: TestSystem::default(),
                vendored: vendored_typeshed_stubs().clone(),
                files: Files::default(),
            }
        }
    }

    impl DbWithTestSystem for TestDb {
        fn test_system(&self) -> &TestSystem {
            &self.system
        }

        fn test_system_mut(&mut self) -> &mut TestSystem {
            &mut self.system
        }
    }

    #[salsa::db]
    impl SourceDb for TestDb {
        fn vendored(&self) -> &VendoredFileSystem {
            &self.vendored
        }

        fn system(&self) -> &dyn System {
            &self.system
        }

        fn files(&self) -> &Files {
            &self.files
        }
    }

    impl Upcast<dyn SemanticDb> for TestDb {
        fn upcast(&self) -> &(dyn SemanticDb + 'static) {
            self
        }
        fn upcast_mut(&mut self) -> &mut (dyn SemanticDb + 'static) {
            self
        }
    }

    impl Upcast<dyn SourceDb> for TestDb {
        fn upcast(&self) -> &(dyn SourceDb + 'static) {
            self
        }
        fn upcast_mut(&mut self) -> &mut (dyn SourceDb + 'static) {
            self
        }
    }

    impl Upcast<dyn ResolverDb> for TestDb {
        fn upcast(&self) -> &(dyn ResolverDb + 'static) {
            self
        }
        fn upcast_mut(&mut self) -> &mut (dyn ResolverDb + 'static) {
            self
        }
    }

    #[salsa::db]
    impl red_knot_module_resolver::Db for TestDb {}
    #[salsa::db]
    impl red_knot_python_semantic::Db for TestDb {}
    #[salsa::db]
    impl Db for TestDb {}

    #[salsa::db]
    impl salsa::Database for TestDb {}
}
