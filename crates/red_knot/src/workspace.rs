use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use anyhow::anyhow;
use rustc_hash::FxHashSet;

use crate::configuration::Configuration;
use crate::db::{Db, RootDatabase};
use crate::lint::{lint_semantic, lint_syntax, Diagnostics};
use ruff_db::files::{system_path_to_file, File};
use ruff_db::program::{Program, RawModuleResolutionSettings, TargetVersion};
use ruff_db::system::walk_directory::WalkState;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_db::Db as _;
use ruff_python_ast::name::Name;
use ruff_python_ast::PySourceType;

/// Metadata about the workspace and its state.
///
/// ## Requires python
/// All projects in a [`Workspace`] must have an overlapping `requires-python` range.
///
/// ## How is [`Workspace`] different from [`Program`]?
/// Workspace is a representation of the entire project structure whereas [`Program`] is the
/// minimal common denominator that allows files to be analysed together. Today, [`Workspace`] to [`Program`] is a
/// 1:1 relationship but this could change in the future, e.g. when a user uses VS Code's workspace feature
/// to open two workspace members in the same window and they use the same search paths.
#[salsa::input(singleton, constructor=new_impl)]
pub struct Workspace {
    #[return_ref]
    path_buf: SystemPathBuf,

    /// The projects (workspace members) by their path.
    ///
    /// Projects can be nested in each other. It's important to take the closest project for a file.
    #[return_ref]
    projects_map: BTreeMap<SystemPathBuf, Project>,

    /// Whether all files in the workspace should be analysed, only the open files, or only a selected set of paths.
    #[return_ref]
    mode: AnalysisMode,

    /// The program for this workspace.
    program: Program,
}

// We can't move `Workspace` into `ruff_db` because `Configurations` depends on all other crates :(

impl Workspace {
    pub fn discover(
        db: &mut RootDatabase,
        path: &SystemPath,
        configuration: &Configuration,
        mode: AnalysisMode,
    ) -> anyhow::Result<Self> {
        fn workspace_dir<'a>(path: &'a SystemPath, system: &dyn System) -> Option<&'a SystemPath> {
            for ancestor in path.ancestors() {
                if system.is_directory(ancestor) {
                    let config_path = ancestor.join("pyproject.toml");

                    if system.is_file(&config_path) {
                        return Some(ancestor);
                    }
                }
            }

            None
        }
        let system = db.system();

        let workspace_path = workspace_dir(path, system).ok_or_else(|| {
            anyhow!("Couldn't find a workspace directory containing a pyproject.toml.")
        })?;

        let mut projects = BTreeMap::new();
        // TODO: Use project.name
        let project_name = Name::new(path.file_name().unwrap_or_default());

        // TODO: load configuration and discover all projects.
        let project = Project::new(
            db,
            path.to_path_buf(),
            project_name,
            OpenFileSet::Empty,
            configuration.target_version,
        );
        projects.insert(path.to_path_buf(), project);

        let settings = RawModuleResolutionSettings {
            extra_paths: configuration.extra_search_paths.clone(),
            workspace_root: workspace_path.to_path_buf(),
            custom_typeshed: configuration.custom_typeshed_dir.clone(),
            site_packages: None,
        };

        let program = Program::new(db, TargetVersion::default(), settings);

        let workspace =
            Workspace::new_impl(db, workspace_path.to_path_buf(), projects, mode, program);

        workspace.reload(db)?;

        Ok(workspace)
    }

    /// Reload the workspace after a configuration change that might impact the workspace structure.
    pub(crate) fn reload(self, db: &mut RootDatabase) -> anyhow::Result<()> {
        let system = db.system();

        let path = self.path(db);

        if !system.is_file(&path.join("pyproject.toml")) {
            return Err(anyhow!("Workspace has been deleted"));
        }

        let mut projects = BTreeMap::new();

        for project in self.projects(db) {
            let path = project.path(db).to_path_buf();
            if system.is_directory(&path) {
                projects.insert(path, project);
            }
        }

        for project in projects.values() {
            project.reload(db, self)?;
        }

        self.set_projects_map(db).to(projects);

        Ok(())
    }

    pub fn path(self, db: &dyn Db) -> &SystemPath {
        &self.path_buf(db)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn check(&self, db: &RootDatabase) -> Result<Vec<String>, salsa::Cancelled> {
        db.with_db(|db| {
            let mut result = Vec::new();

            for project in self.projects(db) {
                result.extend_from_slice(&project.check(db));
            }

            result
        })
    }

    pub fn check_project(
        &self,
        db: &RootDatabase,
        project: &Project,
    ) -> Result<Vec<String>, salsa::Cancelled> {
        db.with_db(|db| project.check(db))
    }

    /// Returns the closest project that contains the given path.
    pub fn project(&self, db: &dyn Db, path: impl AsRef<SystemPath>) -> Option<Project> {
        let path = path.as_ref();
        let (closest_path, closest_project) = self
            .projects_map(db)
            .range(..=path.to_path_buf())
            .next_back()?;

        if path.starts_with(closest_path) {
            Some(*closest_project)
        } else {
            None
        }
    }

    pub fn projects(self, db: &dyn Db) -> impl Iterator<Item = Project> + '_ {
        self.projects_map(db).values().copied()
    }
}

#[salsa::input]
pub struct Project {
    #[return_ref]
    path_buf: SystemPathBuf,

    #[return_ref]
    name: Name,

    #[return_ref]
    open_files_set: OpenFileSet,

    /// TODO: Change this to requires-python. This should most likely be stored on the configuration.
    target_version: TargetVersion,
}

impl Project {
    fn reload(self, db: &mut dyn Db, workspace: Workspace) -> anyhow::Result<()> {
        let open_paths = self.open_paths(db, workspace.mode(db));

        let Some((first, rest)) = open_paths.split_first() else {
            return Ok(());
        };

        let mut builder = db.system().walk_directory(first);

        for path in rest {
            builder.add(path);
        }
        // TODO: Respect the workspace's gitignore setting

        let paths = std::sync::Mutex::new(Vec::default());

        builder.run(|| {
            Box::new(|entry| {
                match entry {
                    Ok(entry) => {
                        if entry.path().extension().is_some_and(|extension| {
                            PySourceType::try_from_extension(extension).is_some()
                        }) {
                            // If it is a file, add it to the open files.
                            let mut paths = paths.lock().unwrap();
                            paths.push(entry.into_path());
                        }

                        WalkState::Continue
                    }
                    Err(error) => {
                        eprintln!("Error walking directory: {error}");
                        WalkState::Continue
                    }
                }
            })
        });

        let open_files: FxHashSet<_> = paths
            .into_inner()
            .unwrap()
            .into_iter()
            .flat_map(|path| system_path_to_file(db.upcast(), path))
            .collect();

        self.set_open_files(db, open_files);

        Ok(())
    }

    pub fn check(self, db: &dyn Db) -> Vec<String> {
        let mut result = Vec::new();

        for open_file in self.open_files(db) {
            result.extend_from_slice(&self.check_file_impl(db, open_file));
        }

        result
    }

    #[tracing::instrument(level = "debug", skip(self, db))]
    pub fn check_file(self, db: &dyn Db, file: File) -> Option<Diagnostics> {
        // TODO: Which methods should be wrapped by Result
        if self.is_open(db, file) {
            Some(self.check_file_impl(db, file))
        } else {
            None
        }
    }

    fn check_file_impl(self, db: &dyn Db, file: File) -> Diagnostics {
        let mut diagnostics = Vec::new();
        diagnostics.extend_from_slice(lint_syntax(db, file));
        diagnostics.extend_from_slice(lint_semantic(db, file));
        Diagnostics::from(diagnostics)
    }

    pub fn path(self, db: &dyn Db) -> &SystemPath {
        &self.path_buf(db)
    }

    pub fn open_files(self, db: &dyn Db) -> impl Iterator<Item = File> + '_ {
        let files = match self.open_files_set(db) {
            OpenFileSet::Set(set) => Some(set.iter().copied()),
            OpenFileSet::Empty => None,
        };

        files.into_iter().flatten()
    }

    pub fn open_file(self, db: &mut dyn Db, file: File) {
        assert!(file
            .path(db.upcast())
            .as_system_path()
            .is_some_and(|path| path.starts_with(self.path(db))));

        self.with_open_files_mut(db, |files| {
            files.insert(file);
        });
    }

    pub fn is_open(&self, db: &dyn Db, file: File) -> bool {
        self.open_files_set(db).contains(file)
    }

    pub fn close_file(self, db: &mut dyn Db, file: File) -> bool {
        self.with_open_files_mut(db, |files| files.remove(&file))
    }

    pub fn set_open_files(self, db: &mut dyn Db, files: FxHashSet<File>) {
        self.set_open_files_set(db).to(OpenFileSet::from(files));
    }

    fn with_open_files_mut<F, R>(self, db: &mut dyn Db, f: F) -> R
    where
        F: FnOnce(&mut FxHashSet<File>) -> R,
    {
        let open_files = self.open_files_set(db);

        match open_files {
            OpenFileSet::Empty => {
                let mut files = FxHashSet::default();
                let result = f(&mut files);
                self.set_open_files_set(db).to(OpenFileSet::from(files));
                result
            }
            OpenFileSet::Set(open_files) => {
                let mut open_files = open_files.clone();

                // Set the open files to an empty set. This causes Salsa to cancel any pending query
                // and removes Salsa's reference to `open_files`, leaving exactly one reference to `open_files`,
                // the one we hold. This means calling `get_mut` on `open_files` will always return `Some`.
                self.set_open_files_set(db).to(OpenFileSet::Empty);
                let files = Arc::get_mut(&mut open_files).unwrap();
                let result = f(files);

                self.set_open_files_set(db)
                    .to(OpenFileSet::from(open_files));
                result
            }
        }
    }

    fn open_paths<'a>(self, db: &'a dyn Db, mode: &'a AnalysisMode) -> &'a [SystemPathBuf] {
        match mode {
            AnalysisMode::Workspace => std::slice::from_ref(self.path_buf(db)),
            AnalysisMode::OpenFiles => &[],
            AnalysisMode::Paths(paths) => paths.as_slice(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum AnalysisMode {
    /// Analyse and show diagnostics for the entire workspace.
    Workspace,
    /// Analyse and show diagnostics for open files only.
    OpenFiles,
    /// Analyse and show diagnostics for files in the given paths only.
    Paths(Vec<SystemPathBuf>),
}

impl AnalysisMode {
    pub const fn is_workspace(&self) -> bool {
        matches!(self, Self::Workspace)
    }

    pub const fn is_open_files(&self) -> bool {
        matches!(self, Self::OpenFiles)
    }
}

impl Default for AnalysisMode {
    fn default() -> Self {
        Self::Workspace
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OpenFileSet {
    Set(Arc<FxHashSet<File>>),
    Empty,
}

impl OpenFileSet {
    fn contains(&self, file: File) -> bool {
        match self {
            OpenFileSet::Set(set) => set.contains(&file),
            OpenFileSet::Empty => false,
        }
    }
}

impl From<FxHashSet<File>> for OpenFileSet {
    fn from(value: FxHashSet<File>) -> Self {
        if value.is_empty() {
            OpenFileSet::Empty
        } else {
            OpenFileSet::Set(Arc::new(value))
        }
    }
}

impl From<Arc<FxHashSet<File>>> for OpenFileSet {
    fn from(value: Arc<FxHashSet<File>>) -> Self {
        if value.is_empty() {
            OpenFileSet::Empty
        } else {
            OpenFileSet::Set(value)
        }
    }
}
