use crate::dependency::DependencyMetadata;
use crate::lint::{LintRegistry, RuleSelection};
use crate::{AnalysisSettings, PythonVersionWithSource};
use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::File;
use ty_python_core::{Db as PythonCoreDb, ProgramFile};

/// Database giving access to semantic information about a Python program.
#[salsa::db]
pub trait Db: PythonCoreDb {
    fn check_file(&self, file: File) -> Vec<Diagnostic>;

    /// Returns the program file for `file`.
    fn program_file(&self, file: File) -> ProgramFile<'_>;

    /// Resolves structural inputs before controlled semantic execution begins.
    ///
    /// Hosts with lazy project or environment state must materialize it here. This runs inside
    /// the preparation boundary, which permits structural queries but rejects type inference.
    fn prepare_analysis_environment(&self, file: File) -> ProgramFile<'_> {
        let program_file = self.program_file(file);
        self.prepare_analysis_file_settings(file);
        program_file
    }

    /// Materializes host settings for a physical file without selecting its program.
    ///
    /// Imported files use the importing program even when the host would select a different
    /// program for an independent check. This runs inside the same preparation boundary as
    /// [`Db::prepare_analysis_environment`].
    fn prepare_analysis_file_settings(&self, file: File) {
        self.rule_selection(file);
        self.analysis_settings(file);
        self.should_check_file(file);
        self.is_open_file(file);
    }

    /// Certifies already prepared host reads while idle, without executing missing queries.
    fn prepare_analysis_host_reads(
        &self,
        _file: File,
    ) -> Result<
        std::rc::Rc<dyn crate::prepared_host::PreparedHostFileReads<'_> + '_>,
        salsa::prepared_source_probe::PreparationError,
    > {
        Err(salsa::prepared_source_probe::PreparationError::UnsupportedDependency)
    }

    /// Returns the Python version and its configuration source for `file`.
    fn python_version_with_source(&self, file: File) -> &PythonVersionWithSource;

    /// Resolves the rule selection for a given file.
    fn rule_selection(&self, file: File) -> &RuleSelection;

    fn lint_registry(&self) -> &LintRegistry;

    fn analysis_settings(&self, file: File) -> &AnalysisSettings;

    /// Returns the package manager's dependency information for this file.
    fn dependency_metadata(&self, file: File) -> Option<&DependencyMetadata>;

    /// Whether ty is running with logging verbosity INFO or higher (`-v` or more).
    fn verbose(&self) -> bool;

    /// Returns `true` if `file` is open in the editor.
    ///
    /// Expected types for string-literal completions are only collected for open files.
    fn is_open_file(&self, file: File) -> bool;

    fn dyn_clone(&self) -> Box<dyn Db>;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use std::rc::Rc;
    use std::sync::{Arc, Mutex};

    use anyhow::Context;
    use salsa::Setter;
    use salsa::execution_probe::{
        BorrowOrCopy, Demand, PreparedSourceMemo, RunResult, TaskEndpoint,
    };
    use salsa::prepared_source_probe::{PreparationError, Stamp};
    use ty_python_core::platform::PythonPlatform;

    use crate::prepared_host::{
        PreparedHostFileReads, admit_host_task_setup, local_with_fixed_transfers_at,
    };
    use crate::{ProgramEnvironment, check_file_unwrap, default_lint_registry};
    use ruff_db::Db as SourceDb;
    use ruff_db::files::Files;
    use ruff_db::system::{
        DbWithTestSystem, DbWithWritableSystem as _, System, SystemPath, SystemPathBuf, TestSystem,
    };
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_python_ast::PythonVersion;
    use ty_module_resolver::{Db as ModuleResolverDb, SearchPathSettings};
    use ty_python_core::TestProgramDb;
    use ty_python_core::program::{FallibleStrategy, ProgramSettings};
    use ty_site_packages::{PythonVersionSource, PythonVersionWithSource};

    type Events = Arc<Mutex<Vec<salsa::Event>>>;
    type EventCallback = Box<dyn Fn(&salsa::EventKind) + Send + Sync + 'static>;

    #[salsa::input(singleton, field_requests=read_fields)]
    struct TestAnalysisSettings {
        #[returns(ref)]
        settings: AnalysisSettings,
    }

    #[salsa::db]
    #[derive(Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        events: Events,
        rule_selection: Arc<RuleSelection>,
        open_files: rustc_hash::FxHashSet<File>,
        program_settings: ProgramSettings,
    }

    impl TestDb {
        fn new(event_callback: Option<EventCallback>) -> Self {
            let events = Events::default();
            let vendored = ty_vendored::file_system().clone();
            let program_settings = ProgramSettings::empty(&vendored);
            let db = Self {
                storage: salsa::Storage::new(Some(Box::new({
                    let events = events.clone();
                    move |event| {
                        if let Some(callback) = &event_callback {
                            callback(&event.kind);
                        }
                        crate::types::observe_constructor_probe_event(&event.kind);
                        tracing::trace!("event: {event:?}");
                        let mut events = events.lock().unwrap();
                        events.push(event);
                    }
                }))),
                system: TestSystem::default(),
                vendored,
                events,
                files: Files::default(),
                rule_selection: Arc::new(RuleSelection::from_registry(default_lint_registry())),
                open_files: rustc_hash::FxHashSet::default(),
                program_settings,
            };
            let _settings = TestAnalysisSettings::new(&db, AnalysisSettings::default());
            db
        }

        pub(crate) fn set_analysis_settings(&mut self, settings: AnalysisSettings) {
            TestAnalysisSettings::get(self)
                .set_settings(self)
                .to(settings);
        }

        pub(crate) fn python_version(&self) -> PythonVersion {
            self.program().python_version(self)
        }

        pub(crate) fn program_environment(&self) -> ProgramEnvironment<'_> {
            ProgramEnvironment::from_program(self.program())
        }

        /// Marks `file` as open in the editor.
        ///
        /// This is untracked state: open a file before running any queries.
        pub(crate) fn open_file(&mut self, file: File) {
            self.open_files.insert(file);
        }

        /// Takes the salsa events.
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let mut events = self.events.lock().unwrap();

            std::mem::take(&mut *events)
        }

        /// Clears the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        pub(crate) fn clear_salsa_events(&mut self) {
            self.take_salsa_events();
        }
    }

    struct PreparedTestHostReads<'db> {
        db: &'db TestDb,
        file: File,
        stamp: Stamp,
        analysis_settings: TestAnalysisSettings,
    }

    impl<'db> PreparedHostFileReads<'db> for PreparedTestHostReads<'db> {
        fn check_current(&self) -> Result<(), PreparationError> {
            PreparedSourceMemo::<()>::check_idle(self.db)
                .map_err(|_| PreparationError::InvalidDependency)?;
            if self.stamp.belongs_to(self.db) {
                Ok(())
            } else {
                Err(PreparationError::ChangedDatabaseStamp)
            }
        }

        fn should_check_file<'run>(
            self: Rc<Self>,
            endpoint: TaskEndpoint<'run, 'db>,
        ) -> RunResult<Demand<bool>>
        where
            'db: 'run,
        {
            endpoint.admit_work(24)?;
            let task = endpoint.clone();
            endpoint.demand(move || async move {
                let path = task
                    .read_field(self.file.read_fields(self.db).path(), &BorrowOrCopy)
                    .await;
                Ok(task
                    .local_call(|| {
                        task.admit_work(2)?;
                        task.check_completion()?;
                        Ok(!path.is_vendored_path())
                    })
                    .await)
            })
        }

        fn rule_selection<'run>(
            self: Rc<Self>,
            endpoint: TaskEndpoint<'run, 'db>,
        ) -> RunResult<Demand<&'db RuleSelection>>
        where
            'db: 'run,
        {
            endpoint.admit_work(24)?;
            let task = endpoint.clone();
            endpoint.demand(move || async move {
                Ok(task
                    .local_call(|| {
                        task.admit_work(2)?;
                        task.check_completion()?;
                        Ok(&*self.db.rule_selection)
                    })
                    .await)
            })
        }

        fn verbose<'run>(
            self: Rc<Self>,
            endpoint: TaskEndpoint<'run, 'db>,
        ) -> RunResult<Demand<bool>>
        where
            'db: 'run,
        {
            let make = |host: Rc<Self>, child: TaskEndpoint<'run, 'db>| move || async move {
                local_with_fixed_transfers_at(&child, 3, 0, || crate::Db::verbose(host.db))
                    .await
            };
            admit_host_task_setup(&endpoint, &make)?;
            let child = endpoint.clone();
            endpoint.demand(make(self, child))
        }

        fn analysis_settings<'run>(
            self: Rc<Self>,
            endpoint: TaskEndpoint<'run, 'db>,
        ) -> RunResult<Demand<&'db AnalysisSettings>>
        where
            'db: 'run,
        {
            endpoint.admit_work(24)?;
            let task = endpoint.clone();
            endpoint.demand(move || async move {
                Ok(task
                    .read_field(
                        self.analysis_settings
                            .read_fields(task.field_request_context())
                            .settings(),
                        &BorrowOrCopy,
                    )
                    .await)
            })
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

    #[salsa::db]
    impl ty_python_core::Db for TestDb {
        fn should_check_file(&self, file: File) -> bool {
            !file.path(self).is_vendored_path()
        }
    }

    #[salsa::db]
    impl TestProgramDb for TestDb {
        fn program_settings(&self) -> &ProgramSettings {
            &self.program_settings
        }
    }

    #[salsa::db]
    impl Db for TestDb {
        fn prepare_analysis_host_reads(
            &self,
            file: File,
        ) -> Result<Rc<dyn PreparedHostFileReads<'_> + '_>, PreparationError> {
            PreparedSourceMemo::<()>::check_idle(self)
                .map_err(|_| PreparationError::InvalidDependency)?;
            Ok(Rc::new(PreparedTestHostReads {
                db: self,
                file,
                stamp: Stamp::current(self),
                analysis_settings: TestAnalysisSettings::try_get(self)
                    .ok_or(PreparationError::InvalidDependency)?,
            }))
        }

        fn check_file(&self, file: File) -> Vec<Diagnostic> {
            if !self.should_check_file(file) {
                return Vec::new();
            }

            check_file_unwrap(self, self.program_file(file))
        }

        fn program_file(&self, file: File) -> ProgramFile<'_> {
            self.program().program_file(self, file)
        }

        fn python_version_with_source(&self, _file: File) -> &PythonVersionWithSource {
            &self.program_settings.python_version
        }

        fn rule_selection(&self, _file: File) -> &RuleSelection {
            &self.rule_selection
        }

        fn lint_registry(&self) -> &LintRegistry {
            default_lint_registry()
        }

        fn analysis_settings(&self, _file: File) -> &AnalysisSettings {
            TestAnalysisSettings::get(self).settings(self)
        }

        fn dependency_metadata(&self, _file: File) -> Option<&DependencyMetadata> {
            None
        }

        fn verbose(&self) -> bool {
            false
        }

        fn is_open_file(&self, file: File) -> bool {
            self.open_files.contains(&file)
        }

        fn dyn_clone(&self) -> Box<dyn crate::Db> {
            Box::new(self.clone())
        }
    }

    #[salsa::db]
    impl ModuleResolverDb for TestDb {}

    #[salsa::db]
    impl salsa::Database for TestDb {}

    pub(crate) struct TestDbBuilder<'a> {
        /// Target Python version
        python_version: PythonVersion,
        /// Target Python platform
        python_platform: PythonPlatform,
        /// Roots containing first-party modules.
        src_roots: Vec<SystemPathBuf>,
        custom_typeshed: Option<SystemPathBuf>,
        /// Path and content pairs for files that should be present
        files: Vec<(&'a str, &'a str)>,
        /// Whether module resolution should include packages from the synthetic virtual environment.
        third_party_packages: bool,
        rule_selection: Option<RuleSelection>,
        event_callback: Option<EventCallback>,
    }

    impl<'a> TestDbBuilder<'a> {
        pub(crate) fn new() -> Self {
            Self {
                python_version: PythonVersion::default(),
                python_platform: PythonPlatform::default(),
                src_roots: vec![SystemPathBuf::from("/src")],
                custom_typeshed: None,
                files: vec![],
                third_party_packages: false,
                rule_selection: None,
                event_callback: None,
            }
        }

        pub(crate) fn with_python_version(mut self, version: PythonVersion) -> Self {
            self.python_version = version;
            self
        }

        pub(crate) fn with_python_platform(mut self, platform: PythonPlatform) -> Self {
            self.python_platform = platform;
            self
        }

        pub(crate) fn with_src_roots(mut self, src_roots: Vec<SystemPathBuf>) -> Self {
            self.src_roots = src_roots;
            self
        }

        pub(crate) fn with_custom_typeshed(mut self, path: SystemPathBuf) -> Self {
            self.custom_typeshed = Some(path);
            self
        }

        pub(crate) fn with_rule_selection(mut self, selection: RuleSelection) -> Self {
            self.rule_selection = Some(selection);
            self
        }

        pub(crate) fn with_salsa_event_callback(
            mut self,
            callback: impl Fn(&salsa::EventKind) + Send + Sync + 'static,
        ) -> Self {
            self.event_callback = Some(Box::new(callback));
            self
        }

        pub(crate) fn with_file(
            mut self,
            path: &'a (impl AsRef<SystemPath> + ?Sized),
            content: &'a str,
        ) -> Self {
            self.files.push((path.as_ref().as_str(), content));
            self
        }

        /// Makes packages installed in the synthetic virtual environment available for imports.
        ///
        /// Files under `/.venv/lib/python3.13/site-packages` are treated as third-party modules,
        /// mirroring the import roots discovered from a project's configured Python environment.
        pub(crate) fn with_third_party_packages(mut self) -> Self {
            self.third_party_packages = true;
            self
        }

        pub(crate) fn build(self) -> anyhow::Result<TestDb> {
            let mut db = TestDb::new(self.event_callback);

            if let Some(selection) = self.rule_selection {
                db.rule_selection = Arc::new(selection);
            }

            for src_root in &self.src_roots {
                db.memory_file_system().create_directory_all(src_root)?;
            }

            let site_packages = SystemPathBuf::from("/.venv/lib/python3.13/site-packages");
            if self.third_party_packages {
                db.memory_file_system()
                    .create_directory_all(&site_packages)?;
            }

            db.write_files(self.files)
                .context("Failed to write test files")?;

            let mut search_path_settings = if self.third_party_packages {
                SearchPathSettings {
                    src_roots: self.src_roots,
                    site_packages_paths: vec![site_packages],
                    ..SearchPathSettings::empty()
                }
            } else {
                SearchPathSettings::new(self.src_roots)
            };
            search_path_settings.custom_typeshed = self.custom_typeshed;

            let program_settings = ProgramSettings {
                python_version: PythonVersionWithSource {
                    version: self.python_version,
                    source: PythonVersionSource::default(),
                },
                python_platform: self.python_platform,
                search_paths: search_path_settings
                    .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
                    .context("Invalid search path settings")?,
            };
            program_settings.search_paths.try_register_static_roots(&db);
            db.program_settings = program_settings;

            Ok(db)
        }
    }

    pub(crate) fn setup_db() -> TestDb {
        TestDbBuilder::new().build().expect("valid TestDb setup")
    }
}
