use std::panic::RefUnwindSafe;
use std::sync::Arc;

use salsa::plumbing::ZalsaDatabase;
use salsa::{Cancelled, Event};

use crate::workspace::{check_file, Workspace, WorkspaceMetadata};
use red_knot_python_semantic::{Db as SemanticDb, Program};
use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::{File, Files};
use ruff_db::system::System;
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Upcast};

mod changes;

#[salsa::db]
pub trait Db: SemanticDb + Upcast<dyn SemanticDb> {
    fn workspace(&self) -> Workspace;
}

#[salsa::db]
pub struct RootDatabase {
    workspace: Option<Workspace>,
    storage: salsa::Storage<RootDatabase>,
    files: Files,
    system: Arc<dyn System + Send + Sync + RefUnwindSafe>,
}

impl RootDatabase {
    pub fn new<S>(workspace: WorkspaceMetadata, system: S) -> anyhow::Result<Self>
    where
        S: System + 'static + Send + Sync + RefUnwindSafe,
    {
        let mut db = Self {
            workspace: None,
            storage: salsa::Storage::default(),
            files: Files::default(),
            system: Arc::new(system),
        };

        // Initialize the `Program` singleton
        Program::from_settings(&db, workspace.settings().program())?;

        db.workspace = Some(Workspace::from_metadata(&db, workspace));

        Ok(db)
    }

    /// Checks all open files in the workspace and its dependencies.
    pub fn check(&self) -> Result<Vec<Box<dyn Diagnostic>>, Cancelled> {
        self.with_db(|db| db.workspace().check(db))
    }

    pub fn check_file(&self, file: File) -> Result<Vec<Box<dyn Diagnostic>>, Cancelled> {
        let _span = tracing::debug_span!("check_file", file=%file.path(self)).entered();

        self.with_db(|db| check_file(db, file))
    }

    /// Returns a mutable reference to the system.
    ///
    /// WARNING: Triggers a new revision, canceling other database handles. This can lead to deadlock.
    pub fn system_mut(&mut self) -> &mut dyn System {
        // TODO: Use a more official method to cancel other queries.
        // https://salsa.zulipchat.com/#narrow/stream/333573-salsa-3.2E0/topic/Expose.20an.20API.20to.20cancel.20other.20queries
        let _ = self.zalsa_mut();

        Arc::get_mut(&mut self.system).unwrap()
    }

    pub(crate) fn with_db<F, T>(&self, f: F) -> Result<T, Cancelled>
    where
        F: FnOnce(&RootDatabase) -> T + std::panic::UnwindSafe,
    {
        Cancelled::catch(|| f(self))
    }

    #[must_use]
    pub fn snapshot(&self) -> Self {
        Self {
            workspace: self.workspace,
            storage: self.storage.clone(),
            files: self.files.snapshot(),
            system: Arc::clone(&self.system),
        }
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

#[salsa::db]
impl SemanticDb for RootDatabase {
    fn is_file_open(&self, file: File) -> bool {
        let Some(workspace) = &self.workspace else {
            return false;
        };

        workspace.is_file_open(self, file)
    }
}

#[salsa::db]
impl SourceDb for RootDatabase {
    fn vendored(&self) -> &VendoredFileSystem {
        red_knot_vendored::file_system()
    }

    fn system(&self) -> &dyn System {
        &*self.system
    }

    fn files(&self) -> &Files {
        &self.files
    }
}

#[salsa::db]
impl salsa::Database for RootDatabase {
    fn salsa_event(&self, event: &dyn Fn() -> Event) {
        if !tracing::enabled!(tracing::Level::TRACE) {
            return;
        }

        let event = event();
        if matches!(event.kind, salsa::EventKind::WillCheckCancellation { .. }) {
            return;
        }

        tracing::trace!("Salsa event: {event:?}");
    }
}

#[salsa::db]
impl Db for RootDatabase {
    fn workspace(&self) -> Workspace {
        self.workspace.unwrap()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use salsa::Event;

    use red_knot_python_semantic::Db as SemanticDb;
    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem, System, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_db::{Db as SourceDb, Upcast};

    use crate::db::Db;
    use crate::workspace::Workspace;

    #[salsa::db]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        events: std::sync::Arc<std::sync::Mutex<Vec<salsa::Event>>>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                system: TestSystem::default(),
                vendored: red_knot_vendored::file_system().clone(),
                files: Files::default(),
                events: Arc::default(),
            }
        }
    }

    impl TestDb {
        /// Takes the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let inner = Arc::get_mut(&mut self.events).expect("no pending salsa snapshots");

            let events = inner.get_mut().unwrap();
            std::mem::take(&mut *events)
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

    #[salsa::db]
    impl red_knot_python_semantic::Db for TestDb {
        fn is_file_open(&self, file: ruff_db::files::File) -> bool {
            !file.path(self).is_vendored_path()
        }
    }

    #[salsa::db]
    impl Db for TestDb {
        fn workspace(&self) -> Workspace {
            todo!()
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {
        fn salsa_event(&self, event: &dyn Fn() -> Event) {
            let mut events = self.events.lock().unwrap();
            events.push(event());
        }
    }
}
