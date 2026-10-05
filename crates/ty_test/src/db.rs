use crate::config::{Analysis, Rules, ScriptOptions};
use camino::{Utf8Component, Utf8PathBuf};
use ruff_db::Db as SourceDb;
use ruff_db::diagnostic::{Diagnostic, Severity};
use ruff_db::files::{File, Files};
use ruff_db::source::source_text;
use ruff_db::system::{
    DbWithWritableSystem, InMemorySystem, OsSystem, System, SystemPath, SystemPathBuf, WhichResult,
    WritableSystem,
};
use ruff_db::vendored::VendoredFileSystem;
use ruff_notebook::{Notebook, NotebookError};
use salsa::Setter as _;
use salsa::execution_probe::{
    BorrowOrCopy, Demand, FieldReadProfile, FieldReturnMode, NativeValueQuote, PreparedSourceMemo,
    RunError, RunResult, TaskEndpoint,
};
use salsa::prepared_source_probe::PreparationError;
use std::borrow::Cow;
use std::future::{Future, ready};
use std::rc::Rc;
use std::sync::Arc;
use tempfile::TempDir;
use ty_module_resolver::ModuleGlobSetBuilder;
use ty_python_core::program::ProgramSettings;
use ty_python_core::{Db as _, ProgramFile, TestProgramDb};
use ty_python_semantic::dependency::DependencyMetadata;
use ty_python_semantic::lint::{LintRegistry, RuleSelection};
use ty_python_semantic::prepared_host::{
    PreparedHostFileReads, admit_host_task_setup, boxed_future_with_fixed_transfers_at,
    generated_field_quote,
};
use ty_python_semantic::{
    AnalysisSettings, Db as SemanticDb, PythonVersionWithSource, check_file_unwrap,
    default_lint_registry,
};

#[salsa::db]
#[derive(Clone)]
pub(crate) struct Db {
    storage: salsa::Storage<Self>,
    files: Files,
    system: MdtestSystem,
    vendored: VendoredFileSystem,
    settings: Option<Settings>,
}

impl Db {
    pub(crate) fn setup() -> Self {
        let vendored = ty_vendored::file_system().clone();
        let program_settings = ProgramSettings::empty(&vendored);
        let mut db = Self {
            system: MdtestSystem::in_memory(),
            storage: salsa::Storage::new(Some(Box::new({
                move |event| {
                    tracing::trace!("event: {:?}", event);
                }
            }))),
            vendored,
            files: Files::default(),
            settings: None,
        };

        db.settings = Some(Settings::new(&db, program_settings));
        db
    }

    fn settings(&self) -> Settings {
        self.settings.unwrap()
    }

    pub(crate) fn update_program(&mut self, settings: ProgramSettings) {
        let db_settings = self.settings();
        if db_settings.program(self) != &settings {
            settings.search_paths.try_register_static_roots(self);
            db_settings.set_program(self).to(settings);
        }
    }

    pub(crate) fn set_verbosity(&mut self, verbose: bool) {
        self.settings().set_verbose(self).to(verbose);
    }

    pub(crate) fn update_analysis_options(&mut self, options: Option<&Analysis>) {
        let analysis = mdtest_analysis_settings(options);

        let settings = self.settings();
        if settings.analysis(self) != &analysis {
            settings.set_analysis(self).to(analysis);
        }
    }

    pub(crate) fn update_dependency_metadata(&mut self, metadata: Option<&DependencyMetadata>) {
        let settings = self.settings();
        if settings.dependency_metadata(self).as_ref() != metadata {
            settings.set_dependency_metadata(self).to(metadata.cloned());
        }
    }

    pub(crate) fn update_mdtest_rule_selection(
        &mut self,
        rules: Option<&Rules>,
        required_rule: Option<&str>,
    ) {
        let rule_selection = mdtest_rule_selection(rules, required_rule);

        let settings = self.settings();
        if settings.rule_selection(self) != &rule_selection {
            settings
                .set_rule_selection(self)
                .to(MdtestRuleSelection(rule_selection));
        }
    }

    pub(crate) fn use_os_system_with_temp_dir(&mut self, cwd: SystemPathBuf, temp_dir: TempDir) {
        self.system.with_os(cwd, temp_dir);
        Files::sync_all(self);
    }

    pub(crate) fn use_in_memory_system(&mut self) {
        self.system.with_in_memory();
        Files::sync_all(self);
    }

    pub(crate) fn create_directory_all(&self, path: &SystemPath) -> ruff_db::system::Result<()> {
        self.system.create_directory_all(path)
    }
}

#[salsa::db]
impl SourceDb for Db {
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
impl ty_module_resolver::Db for Db {}

#[salsa::db]
impl ty_python_core::Db for Db {
    fn should_check_file(&self, file: File) -> bool {
        !file.path(self).is_vendored_path()
    }
}

#[salsa::db]
impl SemanticDb for Db {
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
        &self.settings().program(self).python_version
    }

    fn rule_selection(&self, file: File) -> &RuleSelection {
        file_settings(self, file).rules(self)
    }

    fn prepare_analysis_host_reads(
        &self,
        file: File,
    ) -> Result<Rc<dyn PreparedHostFileReads<'_> + '_>, PreparationError> {
        let file_settings = file_settings::prepare_memo(self, file)
            .map_err(|_| PreparationError::InvalidDependency)?;
        let settings = self.settings.ok_or(PreparationError::InvalidDependency)?;
        let prepared = PreparedMdtestHostFileReads {
            db: self,
            file,
            settings,
            file_settings,
        };
        prepared.check_current()?;
        Ok(Rc::new(prepared))
    }

    fn lint_registry(&self) -> &LintRegistry {
        default_lint_registry()
    }

    fn verbose(&self) -> bool {
        self.settings().verbose(self)
    }

    fn is_open_file(&self, _file: File) -> bool {
        false
    }

    fn analysis_settings(&self, file: File) -> &AnalysisSettings {
        file_settings(self, file).analysis(self)
    }

    fn dependency_metadata(&self, file: File) -> Option<&DependencyMetadata> {
        match file_settings(self, file) {
            FileSettings::Global => self.settings().dependency_metadata(self).as_ref(),
            FileSettings::File {
                dependency_metadata,
                ..
            } => dependency_metadata.as_ref(),
        }
    }

    fn dyn_clone(&self) -> Box<dyn SemanticDb> {
        Box::new(self.clone())
    }
}

#[salsa::db]
impl TestProgramDb for Db {
    fn program_settings(&self) -> &ProgramSettings {
        self.settings().program(self)
    }
}

#[salsa::db]
impl salsa::Database for Db {}

impl DbWithWritableSystem for Db {
    fn writable_system(&self) -> ruff_db::system::Result<&dyn WritableSystem> {
        Ok(&self.system)
    }
}

#[salsa::tracked(attempt = CompleteOnly, returns(ref))]
fn file_settings(db: &dyn SemanticDb, file: File) -> FileSettings {
    let source = source_text(db, file);
    if source.is_notebook() {
        return FileSettings::Global;
    }
    let Some(options) = ScriptOptions::from_source(&source) else {
        return FileSettings::Global;
    };

    FileSettings::File {
        rules: MdtestRuleSelection(mdtest_rule_selection(options.rules.as_ref(), None)),
        analysis: mdtest_analysis_settings(options.analysis.as_ref()),
        dependency_metadata: options.dependency_metadata.map(|fixture| fixture.metadata),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FileSettings {
    Global,
    File {
        rules: MdtestRuleSelection,
        analysis: AnalysisSettings,
        dependency_metadata: Option<DependencyMetadata>,
    },
}

impl FileSettings {
    fn rules<'db>(&'db self, db: &'db Db) -> &'db RuleSelection {
        match self {
            Self::Global => db.settings().rule_selection(db),
            Self::File { rules, .. } => rules,
        }
    }

    fn analysis<'db>(&'db self, db: &'db Db) -> &'db AnalysisSettings {
        match self {
            Self::Global => db.settings().analysis(db),
            Self::File { analysis, .. } => analysis,
        }
    }
}

struct PreparedMdtestHostFileReads<'db> {
    db: &'db Db,
    file: File,
    settings: Settings,
    file_settings: PreparedSourceMemo<'db, FileSettings>,
}

impl<'db> PreparedHostFileReads<'db> for PreparedMdtestHostFileReads<'db> {
    fn check_current(&self) -> Result<(), PreparationError> {
        self.file_settings
            .check_current()
            .map_err(|_| PreparationError::InvalidDependency)
    }

    fn should_check_file<'run>(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
    ) -> RunResult<Demand<bool>>
    where
        'db: 'run,
    {
        // The factory retains this Rc and an endpoint handle; demand admits its storage.
        endpoint.admit_work(16)?;
        let child = endpoint.clone();
        endpoint.demand(move || async move {
            let request = child
                .local_call(|| {
                    child.admit_work(2)?;
                    Ok(self.file.read_fields(self.db).path())
                })
                .await;
            let path = child.read_field(request, &BorrowOrCopy).await;
            Ok(child
                .local_call(|| {
                    child.admit_work(2)?;
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
        endpoint.admit_work(16)?;
        let child = endpoint.clone();
        endpoint.demand(move || async move {
            let request = child
                .local_call(|| {
                    child.admit_work(2)?;
                    Ok(file_settings::prepared_read(&self.file_settings))
                })
                .await;
            let settings = child.read_prepared_source(request).await;
            child.local_call(|| child.admit_work(4)).await;
            match settings {
                FileSettings::Global => {
                    let request = child
                        .local_call(|| {
                            child.admit_work(2)?;
                            Ok(self.settings.read_fields(self.db).rule_selection())
                        })
                        .await;
                    Ok(child.read_field(request, &MdtestRuleSelectionRead).await)
                }
                FileSettings::File { rules, .. } => Ok(&rules.0),
            }
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
            let quote = generated_field_quote(
                |settings: Settings, context| settings.read_fields(context),
                |settings: Settings, context| settings.read_fields(context).verbose(),
            );
            let read = boxed_future_with_fixed_transfers_at(&child, quote, || {
                child.read_field(
                    host.settings.read_fields(child.field_request_context()).verbose(),
                    &BorrowOrCopy,
                )
            })
            .await?;
            Ok(read.await)
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
        endpoint.admit_work(16)?;
        let child = endpoint.clone();
        endpoint.demand(move || async move {
            let request = child
                .local_call(|| {
                    child.admit_work(2)?;
                    Ok(file_settings::prepared_read(&self.file_settings))
                })
                .await;
            let settings = child.read_prepared_source(request).await;
            child.local_call(|| child.admit_work(4)).await;
            match settings {
                FileSettings::Global => {
                    let request = child
                        .local_call(|| {
                            child.admit_work(2)?;
                            Ok(self.settings.read_fields(self.db).analysis())
                        })
                        .await;
                    Ok(child.read_field(request, &BorrowOrCopy).await)
                }
                FileSettings::File { analysis, .. } => Ok(analysis),
            }
        })
    }
}

#[salsa::input(debug, field_requests = read_fields)]
struct Settings {
    #[returns(ref)]
    program: ProgramSettings,
    #[default]
    #[returns(ref)]
    analysis: AnalysisSettings,
    #[default]
    #[returns(ref)]
    dependency_metadata: Option<DependencyMetadata>,
    #[default]
    #[returns(deref)]
    rule_selection: MdtestRuleSelection,
    #[default]
    #[returns(copy)]
    verbose: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MdtestRuleSelection(RuleSelection);

impl Default for MdtestRuleSelection {
    fn default() -> Self {
        Self(mdtest_rule_selection(None, None))
    }
}

impl std::ops::Deref for MdtestRuleSelection {
    type Target = RuleSelection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct MdtestRuleSelectionRead;

impl FieldReadProfile<MdtestRuleSelection> for MdtestRuleSelectionRead {
    fn quote<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        _endpoint: &'call TaskEndpoint<'run, 'db>,
        _stored: &'call MdtestRuleSelection,
        mode: FieldReturnMode,
    ) -> impl Future<Output = RunResult<NativeValueQuote>> + 'call {
        ready(if mode == FieldReturnMode::Deref {
            // MdtestRuleSelection::deref only borrows its stored rule selection.
            Ok(NativeValueQuote {
                work: 1 + size_of::<&RuleSelection>(),
                requested_bytes: 0,
                cleanup_work: 0,
            })
        } else {
            Err(RunError::Contract(
                "mdtest rule selection requires its deref return mode",
            ))
        })
    }
}

fn mdtest_analysis_settings(options: Option<&Analysis>) -> AnalysisSettings {
    let Some(options) = options else {
        return AnalysisSettings::default();
    };

    let AnalysisSettings {
        strict_generic_narrowing: strict_generic_narrowing_default,
        strict_equality_semantics: strict_equality_semantics_default,
        respect_type_ignore_comments: respect_type_ignore_comments_default,
        allowed_unresolved_imports: allowed_unresolved_imports_default,
        replace_imports_with_any: replace_imports_with_any_default,
    } = AnalysisSettings::default();

    let allowed_unresolved_imports =
        if let Some(allowed_unresolved_imports) = options.allowed_unresolved_imports.as_deref() {
            let mut builder = ModuleGlobSetBuilder::new();
            for pattern in allowed_unresolved_imports {
                builder
                    .add(pattern)
                    .expect("Invalid `allowed-unresolved-imports` pattern `{pattern}`");
            }
            builder.build().unwrap()
        } else {
            allowed_unresolved_imports_default
        };

    let replace_imports_with_any =
        if let Some(replace_imports_with_any) = options.replace_imports_with_any.as_deref() {
            let mut builder = ModuleGlobSetBuilder::new();
            for pattern in replace_imports_with_any {
                builder
                    .add(pattern)
                    .expect("Invalid `replace-imports-with-any` pattern `{pattern}`");
            }
            builder.build().unwrap()
        } else {
            replace_imports_with_any_default
        };

    AnalysisSettings {
        strict_generic_narrowing: options
            .strict_generic_narrowing
            .unwrap_or(strict_generic_narrowing_default),
        strict_equality_semantics: options
            .strict_equality_semantics
            .unwrap_or(strict_equality_semantics_default),
        respect_type_ignore_comments: options
            .respect_type_ignore_comments
            .unwrap_or(respect_type_ignore_comments_default),
        allowed_unresolved_imports,
        replace_imports_with_any,
    }
}

fn mdtest_rule_selection(rules: Option<&Rules>, required_rule: Option<&str>) -> RuleSelection {
    // In general (as shown by the initialization of `selection` below), we enable even rules that
    // are ignored by default in mdtests so that their behaviour is covered alongside the default
    // rules. There are a few small exceptions to this, however:
    static DISABLED_IN_MDTESTS: &[&str] = &[
        // `missing-override-decorator` is an exception: because it is extremely pedantic we have
        // chosen to keep it opt-in to minimize churn in unrelated tests.
        "missing-override-decorator",
        // `experimental-syntax` is also an exception: we make use of `&` and `~` for intersection and
        // negation types in our tests for better readability.
        "experimental-syntax",
        // The `unsound-*` rules and `redundant-condition-strict` are also exceptions because they
        // are very strict, would result in lots of additional diagnostics in mdtests, and are not
        // the default behaviour we'll show to our users.
        "unsound-assignment",
        "unsound-return-statement",
        "unsound-yield",
        "redundant-condition-strict",
    ];

    let registry = default_lint_registry();
    let mut selection = RuleSelection::all(registry, Severity::Info);

    for rule in DISABLED_IN_MDTESTS {
        let lint = registry
            .get(rule)
            .unwrap_or_else(|error| panic!("Unknown lint rule `{rule}`: {error}"));
        selection.disable(lint);
    }

    if let Some(rules) = rules {
        let set_lint_level =
            |selection: &mut RuleSelection, lint, level| match Severity::try_from(level) {
                Ok(severity) => {
                    selection.enable(lint, severity, ty_python_semantic::lint::LintSource::File);
                }
                Err(()) => selection.disable(lint),
            };

        // If "all" key is present, use it's value as the default for all rules.
        if let Some(level) = rules.get("all") {
            for lint in registry.lints() {
                set_lint_level(&mut selection, *lint, *level);
            }
        }

        // Apply overrides for specific (non-"all") rules.
        for (rule_name, level) in rules {
            if rule_name == "all" {
                continue;
            }

            let lint = registry
                .get(rule_name)
                .unwrap_or_else(|error| panic!("Unknown lint rule `{rule_name}`: {error}"));
            set_lint_level(&mut selection, lint, *level);
        }
    }

    if let Some(required_rule) = required_rule {
        let lint = registry
            .get(required_rule)
            .unwrap_or_else(|error| panic!("Unknown lint rule `{required_rule}`: {error}"));
        selection.enable(
            lint,
            Severity::Info,
            ty_python_semantic::lint::LintSource::File,
        );
    }

    selection
}

#[derive(Debug, Clone)]
pub(crate) struct MdtestSystem(Arc<MdtestSystemInner>);

#[derive(Debug)]
enum MdtestSystemInner {
    InMemory(InMemorySystem),
    Os {
        os_system: OsSystem,
        _temp_dir: TempDir,
    },
}

impl MdtestSystem {
    fn in_memory() -> Self {
        Self(Arc::new(MdtestSystemInner::InMemory(
            InMemorySystem::default(),
        )))
    }

    fn as_system(&self) -> &dyn WritableSystem {
        match &*self.0 {
            MdtestSystemInner::InMemory(system) => system,
            MdtestSystemInner::Os { os_system, .. } => os_system,
        }
    }

    fn with_os(&mut self, cwd: SystemPathBuf, temp_dir: TempDir) {
        self.0 = Arc::new(MdtestSystemInner::Os {
            os_system: OsSystem::new(cwd),
            _temp_dir: temp_dir,
        });
    }

    fn with_in_memory(&mut self) {
        if let MdtestSystemInner::InMemory(in_memory) = &*self.0 {
            in_memory.fs().remove_all();
        } else {
            self.0 = Arc::new(MdtestSystemInner::InMemory(InMemorySystem::default()));
        }
    }

    fn normalize_path<'a>(&self, path: &'a SystemPath) -> Cow<'a, SystemPath> {
        match &*self.0 {
            MdtestSystemInner::InMemory(_) => Cow::Borrowed(path),
            MdtestSystemInner::Os { os_system, .. } => {
                // Make all paths relative to the current directory
                // to avoid writing or reading from outside the temp directory.
                let without_root: Utf8PathBuf = path
                    .components()
                    .skip_while(|component| {
                        matches!(
                            component,
                            Utf8Component::RootDir | Utf8Component::Prefix(..)
                        )
                    })
                    .collect();
                Cow::Owned(os_system.current_directory().join(&without_root))
            }
        }
    }
}

impl System for MdtestSystem {
    fn path_metadata(
        &self,
        path: &SystemPath,
    ) -> ruff_db::system::Result<ruff_db::system::Metadata> {
        self.as_system().path_metadata(&self.normalize_path(path))
    }

    fn canonicalize_path(&self, path: &SystemPath) -> ruff_db::system::Result<SystemPathBuf> {
        let canonicalized = self
            .as_system()
            .canonicalize_path(&self.normalize_path(path))?;

        if let MdtestSystemInner::Os { os_system, .. } = &*self.0 {
            // Make the path relative to the current directory
            Ok(canonicalized
                .strip_prefix(os_system.current_directory())
                .unwrap()
                .to_owned())
        } else {
            Ok(canonicalized)
        }
    }

    fn is_same_file(
        &self,
        first: &SystemPath,
        second: &SystemPath,
    ) -> ruff_db::system::Result<bool> {
        self.as_system()
            .is_same_file(&self.normalize_path(first), &self.normalize_path(second))
    }

    fn read_to_string(&self, path: &SystemPath) -> ruff_db::system::Result<String> {
        self.as_system().read_to_string(&self.normalize_path(path))
    }

    fn read_to_notebook(&self, path: &SystemPath) -> Result<Notebook, NotebookError> {
        self.as_system()
            .read_to_notebook(&self.normalize_path(path))
    }

    fn read_virtual_path_to_string(
        &self,
        path: &ruff_db::system::SystemVirtualPath,
    ) -> ruff_db::system::Result<String> {
        self.as_system().read_virtual_path_to_string(path)
    }

    fn read_virtual_path_to_notebook(
        &self,
        path: &ruff_db::system::SystemVirtualPath,
    ) -> Result<Notebook, NotebookError> {
        self.as_system().read_virtual_path_to_notebook(path)
    }

    fn which(&self, name: &str) -> WhichResult {
        self.as_system().which(name)
    }

    fn current_directory(&self) -> &SystemPath {
        self.as_system().current_directory()
    }

    fn user_config_directory(&self) -> Option<SystemPathBuf> {
        self.as_system().user_config_directory()
    }

    fn cache_dir(&self) -> Option<SystemPathBuf> {
        self.as_system().cache_dir()
    }

    fn read_directory<'a>(
        &'a self,
        path: &SystemPath,
    ) -> ruff_db::system::Result<
        Box<dyn Iterator<Item = ruff_db::system::Result<ruff_db::system::DirectoryEntry>> + 'a>,
    > {
        self.as_system().read_directory(&self.normalize_path(path))
    }

    fn walk_directory(
        &self,
        path: &SystemPath,
    ) -> ruff_db::system::walk_directory::WalkDirectoryBuilder {
        self.as_system().walk_directory(&self.normalize_path(path))
    }

    fn as_writable(&self) -> Option<&dyn WritableSystem> {
        Some(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn dyn_clone(&self) -> Box<dyn System> {
        Box::new(self.clone())
    }
}

impl WritableSystem for MdtestSystem {
    fn create_new_file(&self, path: &SystemPath) -> ruff_db::system::Result<()> {
        self.as_system().create_new_file(&self.normalize_path(path))
    }

    fn write_file_bytes(&self, path: &SystemPath, content: &[u8]) -> ruff_db::system::Result<()> {
        self.as_system()
            .write_file_bytes(&self.normalize_path(path), content)
    }

    fn create_directory_all(&self, path: &SystemPath) -> ruff_db::system::Result<()> {
        self.as_system()
            .create_directory_all(&self.normalize_path(path))
    }

    fn dyn_clone(&self) -> Box<dyn WritableSystem> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod verbose_tests;

#[cfg(test)]
mod tests {
    use salsa::attempt_probe::{AttemptOutcome, ExecutionLimits, try_with_execution_budget};
    use salsa::execution_probe::RegistryBuilder;
    use salsa::prepared_source_probe::{capture, try_with_preparation};

    use super::*;
    use ruff_db::files::system_path_to_file;

    #[test]
    fn host_sealing_rejects_unprepared_settings_without_reading_them() {
        let db = Db::setup();
        let path = SystemPath::new("/test.py");
        db.system.write_file_bytes(path, b"").unwrap();
        let file = system_path_to_file(&db, path).unwrap();
        let captured = capture(&db, || db.prepare_analysis_host_reads(file)).unwrap();
        assert!(matches!(
            captured.value,
            Err(PreparationError::InvalidDependency)
        ));
        assert!(captured.reads.is_empty());
    }

    #[test]
    fn host_root_reads_borrow_global_and_file_rule_selections() {
        for (source, global) in [
            ("", true),
            (
                "# /// script\n# [tool.ty.rules]\n# division-by-zero = 'ignore'\n# ///\n",
                false,
            ),
        ] {
            let db = Db::setup();
            let path = SystemPath::new("/test.py");
            db.system.write_file_bytes(path, source.as_bytes()).unwrap();
            let file = system_path_to_file(&db, path).unwrap();
            try_with_preparation(&db, || db.prepare_analysis_file_settings(file)).unwrap();
            assert_eq!(
                matches!(file_settings(&db, file), FileSettings::Global),
                global
            );
            let expected = db.rule_selection(file);
            let prepared = db.prepare_analysis_host_reads(file).unwrap();
            let prepared = &prepared;
            let settings_key = file_settings::prepare_memo(&db, file)
                .unwrap()
                .database_key();
            let captured = capture(&db, || {
                try_with_execution_budget(
                    &db,
                    ExecutionLimits {
                        semantic_work: 1_000_000,
                        requested_bytes: 1_000_000,
                    },
                    |budget| {
                        RegistryBuilder::with_budget(&db, &budget)?.seal()?.run(
                            |endpoint| async move {
                                let (child, host) = endpoint
                                    .local_call(|| {
                                        endpoint.admit_work(12)?;
                                        Ok((endpoint.clone(), Rc::clone(prepared)))
                                    })
                                    .await;
                                let eligible = endpoint
                                    .child_call(|| async move {
                                        host.should_check_file(child)?.await
                                    })
                                    .await;
                                let (child, host) = endpoint
                                    .local_call(|| {
                                        endpoint.admit_work(12)?;
                                        Ok((endpoint.clone(), Rc::clone(prepared)))
                                    })
                                    .await;
                                let rules = endpoint
                                    .child_call(|| async move { host.rule_selection(child)?.await })
                                    .await;
                                Ok((eligible, rules))
                            },
                        )
                    },
                )
            })
            .unwrap();
            let AttemptOutcome::Complete(Ok((eligible, actual))) = captured.value.unwrap() else {
                panic!("prepared host reads did not complete");
            };
            assert!(eligible);
            assert!(std::ptr::eq(actual, expected));
            assert_eq!(captured.reads.len(), 1);
            assert_eq!(captured.reads[0].key, settings_key);
            assert!(captured.reads[0].parent.is_none());
            prepared.check_current().unwrap();
        }
    }
}
