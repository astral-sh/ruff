use std::fmt::Formatter;
use std::panic::{AssertUnwindSafe, RefUnwindSafe};
use std::sync::Arc;
use std::{cmp, fmt};

pub use self::changes::ChangeResult;
use crate::metadata::settings::file_settings;
use crate::{DEFAULT_LINT_REGISTRY, DummyReporter};
use crate::{ProgressReporter, Project, ProjectMetadata};
use ruff_db::Db as SourceDb;
use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::{File, Files};
use ruff_db::system::System;
use ruff_db::vendored::VendoredFileSystem;
use salsa::plumbing::ZalsaDatabase;
use salsa::{Event, Setter};
use ty_python_semantic::lint::{LintRegistry, RuleSelection};
use ty_python_semantic::{Db as SemanticDb, Program};

mod changes;

#[salsa::db]
pub trait Db: SemanticDb {
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
        let program_settings = project_metadata.to_program_settings(db.system(), db.vendored())?;
        Program::from_settings(&db, program_settings);

        db.project = Some(
            Project::from_metadata(&db, project_metadata)
                .map_err(|error| anyhow::anyhow!("{}", error.pretty(&db)))?,
        );

        Ok(db)
    }

    /// Checks the files in the project and its dependencies as per the project's check mode.
    ///
    /// Use [`set_check_mode`] to update the check mode.
    ///
    /// [`set_check_mode`]: ProjectDatabase::set_check_mode
    pub fn check(&self) -> Vec<Diagnostic> {
        let mut reporter = DummyReporter;
        let reporter = AssertUnwindSafe(&mut reporter as &mut dyn ProgressReporter);
        self.project().check(self, reporter)
    }

    /// Checks the files in the project and its dependencies, using the given reporter.
    ///
    /// Use [`set_check_mode`] to update the check mode.
    ///
    /// [`set_check_mode`]: ProjectDatabase::set_check_mode
    pub fn check_with_reporter(&self, reporter: &mut dyn ProgressReporter) -> Vec<Diagnostic> {
        let reporter = AssertUnwindSafe(reporter);
        self.project().check(self, reporter)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub fn check_file(&self, file: File) -> Vec<Diagnostic> {
        self.project().check_file(self, file)
    }

    /// Set the check mode for the project.
    pub fn set_check_mode(&mut self, mode: CheckMode) {
        tracing::debug!("Updating project to check {mode}");
        self.project().set_check_mode(self).to(mode);
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

    /// Returns a [`SalsaMemoryDump`] that can be use to dump Salsa memory usage information
    /// to the CLI after a typechecker run.
    pub fn salsa_memory_dump(&self) -> SalsaMemoryDump {
        let salsa_db = self as &dyn salsa::Database;

        let mut ingredients = salsa_db.structs_info();
        let mut memos = salsa_db.queries_info().into_iter().collect::<Vec<_>>();

        ingredients.sort_by_key(|ingredient| cmp::Reverse(ingredient.size_of_fields()));
        memos.sort_by_key(|(_, memo)| cmp::Reverse(memo.size_of_fields()));

        let mut total_fields = 0;
        let mut total_metadata = 0;
        for ingredient in &ingredients {
            total_metadata += ingredient.size_of_metadata();
            total_fields += ingredient.size_of_fields();
        }

        let mut total_memo_fields = 0;
        let mut total_memo_metadata = 0;
        for (_, memo) in &memos {
            total_memo_fields += memo.size_of_fields();
            total_memo_metadata += memo.size_of_metadata();
        }

        SalsaMemoryDump {
            total_fields,
            total_metadata,
            total_memo_fields,
            total_memo_metadata,
            ingredients,
            memos,
        }
    }
}

impl std::fmt::Debug for ProjectDatabase {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProjectDatabase")
            .field("project", &self.project)
            .field("files", &self.files)
            .field("system", &self.system)
            .finish_non_exhaustive()
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize))]
pub enum CheckMode {
    /// Checks the open files in the project.
    OpenFiles,

    /// Checks all files in the project, ignoring the open file set.
    ///
    /// This includes virtual files, such as those opened in an editor.
    #[default]
    AllFiles,
}

impl fmt::Display for CheckMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckMode::OpenFiles => write!(f, "open files"),
            CheckMode::AllFiles => write!(f, "all files"),
        }
    }
}

/// Stores memory usage information.
pub struct SalsaMemoryDump {
    total_fields: usize,
    total_metadata: usize,
    total_memo_fields: usize,
    total_memo_metadata: usize,
    ingredients: Vec<salsa::IngredientInfo>,
    memos: Vec<(&'static str, salsa::IngredientInfo)>,
}

#[allow(clippy::cast_precision_loss)]
fn bytes_to_mb(total: usize) -> f64 {
    total as f64 / 1_000_000.
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
impl SalsaMemoryDump {
    /// Returns a short report that provides total memory usage information.
    pub fn display_short(&self) -> impl fmt::Display + '_ {
        struct DisplayShort<'a>(&'a SalsaMemoryDump);

        impl fmt::Display for DisplayShort<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let SalsaMemoryDump {
                    total_fields,
                    total_metadata,
                    total_memo_fields,
                    total_memo_metadata,
                    ref ingredients,
                    ref memos,
                } = *self.0;

                writeln!(f, "=======SALSA SUMMARY=======")?;

                writeln!(
                    f,
                    "TOTAL MEMORY USAGE: {:.2}MB",
                    bytes_to_mb(
                        total_metadata + total_fields + total_memo_fields + total_memo_metadata
                    )
                )?;

                writeln!(
                    f,
                    "    struct metadata = {:.2}MB",
                    bytes_to_mb(total_metadata),
                )?;
                writeln!(f, "    struct fields = {:.2}MB", bytes_to_mb(total_fields))?;
                writeln!(
                    f,
                    "    memo metadata = {:.2}MB",
                    bytes_to_mb(total_memo_metadata),
                )?;
                writeln!(
                    f,
                    "    memo fields = {:.2}MB",
                    bytes_to_mb(total_memo_fields),
                )?;

                writeln!(f, "QUERY COUNT: {}", memos.len())?;
                writeln!(f, "STRUCT COUNT: {}", ingredients.len())?;

                Ok(())
            }
        }

        DisplayShort(self)
    }

    /// Returns a short report that provides fine-grained memory usage information per
    /// Salsa ingredient.
    pub fn display_full(&self) -> impl fmt::Display + '_ {
        struct DisplayFull<'a>(&'a SalsaMemoryDump);

        impl fmt::Display for DisplayFull<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let SalsaMemoryDump {
                    total_fields,
                    total_metadata,
                    total_memo_fields,
                    total_memo_metadata,
                    ref ingredients,
                    ref memos,
                } = *self.0;

                writeln!(f, "=======SALSA STRUCTS=======")?;

                for ingredient in ingredients {
                    writeln!(
                        f,
                        "{:<50} metadata={:<8} fields={:<8} count={}",
                        format!("`{}`", ingredient.debug_name()),
                        format!("{:.2}MB", bytes_to_mb(ingredient.size_of_metadata())),
                        format!("{:.2}MB", bytes_to_mb(ingredient.size_of_fields())),
                        ingredient.count()
                    )?;
                }

                writeln!(f, "=======SALSA QUERIES=======")?;

                for (query_fn, memo) in memos {
                    writeln!(f, "`{query_fn} -> {}`", memo.debug_name())?;

                    writeln!(
                        f,
                        "    metadata={:<8} fields={:<8} count={}",
                        format!("{:.2}MB", bytes_to_mb(memo.size_of_metadata())),
                        format!("{:.2}MB", bytes_to_mb(memo.size_of_fields())),
                        memo.count()
                    )?;
                }

                writeln!(f, "=======SALSA SUMMARY=======")?;
                writeln!(
                    f,
                    "TOTAL MEMORY USAGE: {:.2}MB",
                    bytes_to_mb(
                        total_metadata + total_fields + total_memo_fields + total_memo_metadata
                    )
                )?;

                writeln!(
                    f,
                    "    struct metadata = {:.2}MB",
                    bytes_to_mb(total_metadata),
                )?;
                writeln!(f, "    struct fields = {:.2}MB", bytes_to_mb(total_fields))?;
                writeln!(
                    f,
                    "    memo metadata = {:.2}MB",
                    bytes_to_mb(total_memo_metadata),
                )?;
                writeln!(
                    f,
                    "    memo fields = {:.2}MB",
                    bytes_to_mb(total_memo_fields),
                )?;

                Ok(())
            }
        }

        DisplayFull(self)
    }

    /// Returns a redacted report that provides rounded totals of memory usage, to avoid
    /// overly sensitive diffs in `mypy-primer` runs.
    pub fn display_mypy_primer(&self) -> impl fmt::Display + '_ {
        struct DisplayShort<'a>(&'a SalsaMemoryDump);

        fn round_memory(total: usize) -> usize {
            // Round the number to the nearest power of 1.05. This gives us a
            // 2.5% threshold before the memory usage number is considered to have
            // changed.
            //
            // TODO: Small changes in memory usage may cause the number to be rounded
            // into the next power if it happened to already be close to the threshold.
            // This also means that differences may surface as a result of small changes
            // over time that are unrelated to the current change. Ideally we could compare
            // the exact numbers across runs and compute the difference, but we don't have
            // the infrastructure for that currently.
            const BASE: f64 = 1.05;
            BASE.powf(bytes_to_mb(total).log(BASE).round()) as usize
        }

        impl fmt::Display for DisplayShort<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let SalsaMemoryDump {
                    total_fields,
                    total_metadata,
                    total_memo_fields,
                    total_memo_metadata,
                    ..
                } = *self.0;

                writeln!(f, "=======SALSA SUMMARY=======")?;

                writeln!(
                    f,
                    "TOTAL MEMORY USAGE: ~{}MB",
                    round_memory(
                        total_metadata + total_fields + total_memo_fields + total_memo_metadata
                    )
                )?;

                writeln!(
                    f,
                    "    struct metadata = ~{}MB",
                    round_memory(total_metadata)
                )?;
                writeln!(f, "    struct fields = ~{}MB", round_memory(total_fields))?;
                writeln!(
                    f,
                    "    memo metadata = ~{}MB",
                    round_memory(total_memo_metadata)
                )?;
                writeln!(
                    f,
                    "    memo fields = ~{}MB",
                    round_memory(total_memo_fields)
                )?;

                Ok(())
            }
        }

        DisplayShort(self)
    }
}

#[salsa::db]
impl SemanticDb for ProjectDatabase {
    fn should_check_file(&self, file: File) -> bool {
        self.project
            .is_some_and(|project| project.should_check_file(self, file))
    }

    fn rule_selection(&self, file: File) -> &RuleSelection {
        let settings = file_settings(self, file);
        settings.rules(self)
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
    use ruff_db::files::File;
    use ruff_python_formatter::{Db as FormatDb, PyFormatOptions};

    #[salsa::db]
    impl FormatDb for ProjectDatabase {
        fn format_options(&self, file: File) -> PyFormatOptions {
            let source_ty = file.source_type(self);
            PyFormatOptions::from_source_type(source_ty)
        }
    }
}

#[cfg(any(test, feature = "testing"))]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use ruff_db::Db as SourceDb;
    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem, System, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;
    use ty_python_semantic::Program;
    use ty_python_semantic::lint::{LintRegistry, RuleSelection};

    use crate::DEFAULT_LINT_REGISTRY;
    use crate::db::Db;
    use crate::{Project, ProjectMetadata};

    type Events = Arc<Mutex<Vec<salsa::Event>>>;

    #[salsa::db]
    #[derive(Clone)]
    pub struct TestDb {
        storage: salsa::Storage<Self>,
        events: Events,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        project: Option<Project>,
    }

    impl TestDb {
        pub fn new(project: ProjectMetadata) -> Self {
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

            let project = Project::from_metadata(&db, project).unwrap();
            db.project = Some(project);
            db
        }
    }

    impl TestDb {
        /// Takes the salsa events.
        pub fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
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

    #[salsa::db]
    impl ty_python_semantic::Db for TestDb {
        fn should_check_file(&self, file: ruff_db::files::File) -> bool {
            !file.path(self).is_vendored_path()
        }

        fn rule_selection(&self, _file: ruff_db::files::File) -> &RuleSelection {
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
