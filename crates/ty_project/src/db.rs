use std::panic::{AssertUnwindSafe, RefUnwindSafe};
use std::sync::Arc;

use crate::{DEFAULT_LINT_REGISTRY, DummyReporter};
use crate::{Project, ProjectMetadata, Reporter};
use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::{File, Files};
use ruff_db::system::System;
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::{Db as SourceDb, Upcast};
use salsa::plumbing::ZalsaDatabase;
use salsa::{Cancelled, Event};
use ty_ide::Db as IdeDb;
use ty_python_semantic::lint::{LintRegistry, RuleSelection};
use ty_python_semantic::{Db as SemanticDb, Program};

mod changes;

#[salsa::db]
pub trait Db: SemanticDb + Upcast<dyn SemanticDb> {
    fn project(&self) -> Project;
}

#[salsa::db]
#[derive(Clone)]
pub struct ProjectDatabase {
    project: Option<Project>,
    files: Files,

    // IMPORTANT: Never return clones of `system` outside `ProjectDatabase` (only return references)
    // or the "trick" to get a mutable `Arc` in `Self::system_mut` is no longer guaranteed to work.
    system: Arc<dyn System + Send + Sync + RefUnwindSafe>,

    // IMPORTANT: This field must be the last because we use `zalsa_mut` (drops all other storage references)
    // to drop all other references to the database, which gives us exclusive access to other `Arc`s stored on this db.
    // However, for this to work it's important that the `storage` is dropped AFTER any `Arc` that
    // we try to mutably borrow using `Arc::get_mut` (like `system`).
    storage: salsa::Storage<ProjectDatabase>,
}

impl ProjectDatabase {
    pub fn new<S>(project_metadata: ProjectMetadata, system: S) -> anyhow::Result<Self>
    where
        S: System + 'static + Send + Sync + RefUnwindSafe,
    {
        let mut db = Self {
            project: None,
            storage: salsa::Storage::new(if tracing::enabled!(tracing::Level::TRACE) {
                Some(Box::new({
                    move |event: Event| {
                        if matches!(event.kind, salsa::EventKind::WillCheckCancellation) {
                            return;
                        }

                        tracing::trace!("Salsa event: {event:?}");
                    }
                }))
            } else {
                None
            }),
            files: Files::default(),
            system: Arc::new(system),
        };

        // TODO: Use the `program_settings` to compute the key for the database's persistent
        //   cache and load the cache if it exists.
        //   we may want to have a dedicated method for this?

        // Initialize the `Program` singleton
        let program_settings = project_metadata.to_program_settings(db.system());
        Program::from_settings(&db, program_settings)?;

        db.project = Some(Project::from_metadata(&db, project_metadata));

        Ok(db)
    }

    /// Checks all open files in the project and its dependencies.
    pub fn check(&self) -> Result<Vec<Diagnostic>, Cancelled> {
        let mut reporter = DummyReporter;
        let reporter = AssertUnwindSafe(&mut reporter as &mut dyn Reporter);
        self.with_db(|db| db.project().check(db, reporter))
    }

    /// Checks all open files in the project and its dependencies, using the given reporter.
    pub fn check_with_reporter(
        &self,
        reporter: &mut dyn Reporter,
    ) -> Result<Vec<Diagnostic>, Cancelled> {
        let reporter = AssertUnwindSafe(reporter);
        self.with_db(|db| db.project().check(db, reporter))
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub fn check_file(&self, file: File) -> Result<Vec<Diagnostic>, Cancelled> {
        self.with_db(|db| self.project().check_file(db, file))
    }

    /// Returns a mutable reference to the system.
    ///
    /// WARNING: Triggers a new revision, canceling other database handles. This can lead to deadlock.
    pub fn system_mut(&mut self) -> &mut dyn System {
        // TODO: Use a more official method to cancel other queries.
        // https://salsa.zulipchat.com/#narrow/stream/333573-salsa-3.2E0/topic/Expose.20an.20API.20to.20cancel.20other.20queries
        let _ = self.zalsa_mut();

        Arc::get_mut(&mut self.system)
            .expect("ref count should be 1 because `zalsa_mut` drops all other DB references.")
    }

    pub(crate) fn with_db<F, T>(&self, f: F) -> Result<T, Cancelled>
    where
        F: FnOnce(&ProjectDatabase) -> T + std::panic::UnwindSafe,
    {
        Cancelled::catch(|| f(self))
    }
}

impl Upcast<dyn SemanticDb> for ProjectDatabase {
    fn upcast(&self) -> &(dyn SemanticDb + 'static) {
        self
    }

    fn upcast_mut(&mut self) -> &mut (dyn SemanticDb + 'static) {
        self
    }
}

impl Upcast<dyn SourceDb> for ProjectDatabase {
    fn upcast(&self) -> &(dyn SourceDb + 'static) {
        self
    }

    fn upcast_mut(&mut self) -> &mut (dyn SourceDb + 'static) {
        self
    }
}

impl Upcast<dyn IdeDb> for ProjectDatabase {
    fn upcast(&self) -> &(dyn IdeDb + 'static) {
        self
    }

    fn upcast_mut(&mut self) -> &mut (dyn IdeDb + 'static) {
        self
    }
}

impl Upcast<dyn Db> for ProjectDatabase {
    fn upcast(&self) -> &(dyn Db + 'static) {
        self
    }

    fn upcast_mut(&mut self) -> &mut (dyn Db + 'static) {
        self
    }
}

#[salsa::db]
impl IdeDb for ProjectDatabase {}

#[salsa::db]
impl SemanticDb for ProjectDatabase {
    fn is_file_open(&self, file: File) -> bool {
        let Some(project) = &self.project else {
            return false;
        };

        project.is_file_open(self, file)
    }

    fn rule_selection(&self) -> &RuleSelection {
        self.project().rules(self)
    }

    fn lint_registry(&self) -> &LintRegistry {
        &DEFAULT_LINT_REGISTRY
    }
}

#[salsa::db]
impl SourceDb for ProjectDatabase {
    fn vendored(&self) -> &VendoredFileSystem {
        ty_vendored::file_system()
    }

    fn system(&self) -> &dyn System {
        &*self.system
    }

    fn files(&self) -> &Files {
        &self.files
    }

    fn python_version(&self) -> ruff_python_ast::PythonVersion {
        Program::get(self).python_version(self)
    }
}

#[salsa::db]
impl salsa::Database for ProjectDatabase {}

#[salsa::db]
impl Db for ProjectDatabase {
    fn project(&self) -> Project {
        self.project.unwrap()
    }
}

#[cfg(feature = "format")]
mod format {
    use crate::ProjectDatabase;
    use ruff_db::Upcast;
    use ruff_db::files::File;
    use ruff_python_formatter::{Db as FormatDb, PyFormatOptions};

    #[salsa::db]
    impl FormatDb for ProjectDatabase {
        fn format_options(&self, file: File) -> PyFormatOptions {
            let source_ty = file.source_type(self);
            PyFormatOptions::from_source_type(source_ty)
        }
    }

    impl Upcast<dyn FormatDb> for ProjectDatabase {
        fn upcast(&self) -> &(dyn FormatDb + 'static) {
            self
        }

        fn upcast_mut(&mut self) -> &mut (dyn FormatDb + 'static) {
            self
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem, System, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_db::{Db as SourceDb, Upcast};
    use ty_python_semantic::lint::{LintRegistry, RuleSelection};
    use ty_python_semantic::{Db as SemanticDb, Program};

    use crate::DEFAULT_LINT_REGISTRY;
    use crate::db::Db;
    use crate::{Project, ProjectMetadata};

    type Events = Arc<Mutex<Vec<salsa::Event>>>;

    #[salsa::db]
    #[derive(Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        events: Events,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        project: Option<Project>,
    }

    impl TestDb {
        pub(crate) fn new(project: ProjectMetadata) -> Self {
            let events = Events::default();
            let mut db = Self {
                storage: salsa::Storage::new(Some(Box::new({
                    let events = events.clone();
                    move |event| {
                        let mut events = events.lock().unwrap();
                        events.push(event);
                    }
                }))),
                system: TestSystem::default(),
                vendored: ty_vendored::file_system().clone(),
                files: Files::default(),
                events,
                project: None,
            };

            let project = Project::from_metadata(&db, project);
            db.project = Some(project);
            db
        }
    }

    impl TestDb {
        /// Takes the salsa events.
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let mut events = self.events.lock().unwrap();

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

        fn python_version(&self) -> ruff_python_ast::PythonVersion {
            Program::get(self).python_version(self)
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
    impl ty_python_semantic::Db for TestDb {
        fn is_file_open(&self, file: ruff_db::files::File) -> bool {
            !file.path(self).is_vendored_path()
        }

        fn rule_selection(&self) -> &RuleSelection {
            self.project().rules(self)
        }

        fn lint_registry(&self) -> &LintRegistry {
            &DEFAULT_LINT_REGISTRY
        }
    }

    #[salsa::db]
    impl Db for TestDb {
        fn project(&self) -> Project {
            self.project.unwrap()
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}
}
