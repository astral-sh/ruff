use camino::{Utf8Component, Utf8PathBuf};
use ruff_db::Db as SourceDb;
use ruff_db::diagnostic::Severity;
use ruff_db::files::{File, Files};
use ruff_db::system::{
    CaseSensitivity, DbWithWritableSystem, InMemorySystem, OsSystem, System, SystemPath,
    SystemPathBuf, WritableSystem,
};
use ruff_db::vendored::VendoredFileSystem;
use ruff_notebook::{Notebook, NotebookError};
use salsa::Setter as _;
use std::borrow::Cow;
use std::sync::Arc;
use tempfile::TempDir;
use ty_module_resolver::SearchPaths;
use ty_python_semantic::lint::{LintRegistry, RuleSelection};
use ty_python_semantic::{AnalysisSettings, Db as SemanticDb, Program, default_lint_registry};

use crate::config::Analysis;

#[salsa::db]
#[derive(Clone)]
pub(crate) struct Db {
    storage: salsa::Storage<Self>,
    files: Files,
    system: MdtestSystem,
    vendored: VendoredFileSystem,
    rule_selection: Arc<RuleSelection>,
    settings: Option<Settings>,
}

impl Db {
    pub(crate) fn setup() -> Self {
        let rule_selection = RuleSelection::all(default_lint_registry(), Severity::Info);

        let mut db = Self {
            system: MdtestSystem::in_memory(),
            storage: salsa::Storage::new(Some(Box::new({
                move |event| {
                    tracing::trace!("event: {:?}", event);
                }
            }))),
            vendored: ty_vendored::file_system().clone(),
            files: Files::default(),
            rule_selection: Arc::new(rule_selection),
            settings: None,
        };

        db.settings = Some(Settings::new(&db));
        db
    }

    fn settings(&self) -> Settings {
        self.settings.unwrap()
    }

    pub(crate) fn update_analysis_options(&mut self, options: Option<&Analysis>) {
        let analysis = if let Some(options) = options {
            let AnalysisSettings {
                respect_type_ignore_comments: respect_type_ignore_comments_default,
            } = AnalysisSettings::default();

            AnalysisSettings {
                respect_type_ignore_comments: options
                    .respect_type_ignore_comments
                    .unwrap_or(respect_type_ignore_comments_default),
            }
        } else {
            AnalysisSettings::default()
        };

        let settings = self.settings();
        if settings.analysis(self) != &analysis {
            settings.set_analysis(self).to(analysis);
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

    fn python_version(&self) -> ruff_python_ast::PythonVersion {
        Program::get(self).python_version(self)
    }
}

#[salsa::db]
impl ty_module_resolver::Db for Db {
    fn search_paths(&self) -> &SearchPaths {
        Program::get(self).search_paths(self)
    }
}

#[salsa::db]
impl SemanticDb for Db {
    fn should_check_file(&self, file: File) -> bool {
        !file.path(self).is_vendored_path()
    }

    fn rule_selection(&self, _file: File) -> &RuleSelection {
        &self.rule_selection
    }

    fn lint_registry(&self) -> &LintRegistry {
        default_lint_registry()
    }

    fn verbose(&self) -> bool {
        false
    }

    fn analysis_settings(&self) -> &AnalysisSettings {
        self.settings().analysis(self)
    }
}

#[salsa::db]
impl salsa::Database for Db {}

impl DbWithWritableSystem for Db {
    type System = MdtestSystem;
    fn writable_system(&self) -> &Self::System {
        &self.system
    }
}

#[salsa::input(debug)]
struct Settings {
    #[default]
    #[returns(ref)]
    analysis: AnalysisSettings,
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

    fn path_exists_case_sensitive(&self, path: &SystemPath, prefix: &SystemPath) -> bool {
        self.as_system()
            .path_exists_case_sensitive(&self.normalize_path(path), &self.normalize_path(prefix))
    }

    fn case_sensitivity(&self) -> CaseSensitivity {
        self.as_system().case_sensitivity()
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

    fn glob(
        &self,
        pattern: &str,
    ) -> Result<
        Box<dyn Iterator<Item = Result<SystemPathBuf, ruff_db::system::GlobError>> + '_>,
        ruff_db::system::PatternError,
    > {
        self.as_system()
            .glob(self.normalize_path(SystemPath::new(pattern)).as_str())
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

    fn write_file(&self, path: &SystemPath, content: &str) -> ruff_db::system::Result<()> {
        self.as_system()
            .write_file(&self.normalize_path(path), content)
    }

    fn create_directory_all(&self, path: &SystemPath) -> ruff_db::system::Result<()> {
        self.as_system()
            .create_directory_all(&self.normalize_path(path))
    }
}
