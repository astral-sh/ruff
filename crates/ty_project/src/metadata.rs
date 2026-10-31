use compact_str::CompactString;
use configuration_file::{ConfigurationFile, ConfigurationFileError};
use ruff_db::diagnostic::{Annotation, Diagnostic, Span};
use ruff_db::files::FileRootKind;
use ruff_db::files::system_path_to_file;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_ranged_value::ValueSource;
use std::sync::Arc;
use thiserror::Error;
use ty_combine::Combine;
use ty_python_core::program::{FallibleStrategy, MisconfigurationStrategy, ProgramSettings};
use ty_python_semantic::PythonEnvironment;

use crate::Db;
use crate::metadata::options::{
    EnvironmentOptions, OptionDiagnostic, OptionsContext, ProgramSettingsDiagnostic,
    ToProgramSettingsError, ToSettingsError,
};
use crate::metadata::pyproject::{Project, PyProject, PyProjectError, ResolveRequiresPythonError};
use crate::metadata::settings::Settings;
use crate::metadata::value::RelativePathBuf;
use crate::uv::{self, UseUv, UvWorkspace};
pub use options::Options;
use options::TyTomlError;
mod configuration_file;
pub mod options;
pub mod pyproject;
pub mod python_version;
pub mod settings;
pub mod value;

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct ProjectMetadata {
    name: ProjectName,

    pub(super) root: SystemPathBuf,

    /// The highest-precedence options, such as CLI flags or inline editor configuration.
    #[cfg_attr(test, serde(skip_serializing_if = "Option::is_none"))]
    override_options: Option<Box<Options>>,

    /// The raw (unmerged, unresolved) options from the project's configuration.
    /// When [`Self::config_file_override`] is `None`, then these are the options from the
    /// project's `ty.toml` or `pyproject.toml`. The options come from
    /// the file specified by [`Self::config_file_override`] if it is `Some` (e.g. when using `--config-file <path>`).
    options: Options,

    configuration_source: ConfigurationSource,

    /// The Python environment derived from uv workspace metadata.
    ///
    /// These options have higher precedence than project and user-level configuration.
    #[cfg_attr(test, serde(skip_serializing_if = "Option::is_none"))]
    uv_workspace_options: Option<Box<Options>>,

    /// The user-level configuration path and its options.
    ///
    /// Its options have lower precedence than [`Self::override_options`], [`Self::options`], and
    /// [`Self::uv_workspace_options`], but higher precedence than [`Self::fallback_options`].
    #[cfg_attr(test, serde(skip_serializing_if = "Option::is_none"))]
    user_configuration: Option<Box<(SystemPathBuf, Options)>>,

    /// The lowest-precedence options, such as the editor-selected Python environment.
    #[cfg_attr(test, serde(skip_serializing_if = "Option::is_none"))]
    fallback_options: Option<Box<Options>>,

    #[cfg_attr(test, serde(skip))]
    uv_workspace: UvWorkspace,

    #[cfg_attr(test, serde(skip))]
    use_uv: UseUv,
}

impl ProjectMetadata {
    /// Creates a project with the given name and root that uses the default options.
    pub fn new(name: impl AsRef<str>, root: SystemPathBuf) -> Self {
        Self {
            name: ProjectName::new(name),
            root,
            options: Options::default(),
            configuration_source: ConfigurationSource::Default,
            uv_workspace_options: None,
            override_options: None,
            user_configuration: None,
            fallback_options: None,
            uv_workspace: UvWorkspace::default(),
            use_uv: UseUv::Off,
        }
    }

    /// Loads an explicitly selected configuration file.
    pub fn from_config_file(
        path: SystemPathBuf,
        root: &SystemPath,
        system: &dyn System,
    ) -> Result<Self, ProjectMetadataError> {
        tracing::debug!("Using overridden configuration file at '{path}'");

        let config_file = ConfigurationFile::from_path(path.clone(), system).map_err(|error| {
            ProjectMetadataError::ConfigurationFileError {
                source: Box::new(error),
                path: path.clone(),
            }
        })?;

        let options = config_file.into_options();

        Ok(Self {
            name: ProjectName::new(root.file_name().unwrap_or("root")),
            root: root.to_path_buf(),
            options,
            configuration_source: ConfigurationSource::ConfigFile(path),
            uv_workspace_options: None,
            override_options: None,
            user_configuration: None,
            fallback_options: None,
            uv_workspace: UvWorkspace::default(),
            use_uv: UseUv::from_system(system),
        })
    }

    /// Loads a project from a `pyproject.toml` file.
    fn from_pyproject(
        pyproject: PyProject,
        root: SystemPathBuf,
    ) -> Result<Self, ResolveRequiresPythonError> {
        let configuration_source = if pyproject.ty().is_some() {
            ConfigurationSource::Ty
        } else {
            ConfigurationSource::Pyproject
        };
        let mut metadata = Self::from_options(
            pyproject.tool.and_then(|tool| tool.ty).unwrap_or_default(),
            root,
            pyproject.project.as_ref(),
            &FallibleStrategy,
        )?;
        metadata.configuration_source = configuration_source;
        Ok(metadata)
    }

    /// Loads a project from a set of options with an optional pyproject-project table.
    pub fn from_options<Strategy: MisconfigurationStrategy>(
        mut options: Options,
        root: SystemPathBuf,
        project: Option<&Project>,
        strategy: &Strategy,
    ) -> Result<Self, Strategy::Error<ResolveRequiresPythonError>> {
        let name = project
            .and_then(|project| project.name.as_deref())
            .map(|name| ProjectName::new(&**name))
            .unwrap_or_else(|| ProjectName::new(root.file_name().unwrap_or("root")));

        if let Some(project) = project {
            // If the `options` don't specify a python version but the `project.requires-python` field is set,
            // use that as a lower bound instead.
            strategy.fallback(
                options.apply_requires_python(project.requires_python.as_ref()),
                |error| tracing::debug!("skipping invalid requires_python lower bound: {error}"),
            )?;
        }

        Ok(Self {
            name,
            root,
            options,
            configuration_source: ConfigurationSource::Default,
            uv_workspace_options: None,
            override_options: None,
            user_configuration: None,
            fallback_options: None,
            uv_workspace: UvWorkspace::default(),
            use_uv: UseUv::Off,
        })
    }

    /// Discovers the closest project at `path` and returns its metadata.
    ///
    /// The algorithm traverses upwards in the `path`'s ancestor chain and uses the following precedence
    /// to resolve the project's root.
    ///
    /// 1. The closest `pyproject.toml` with a `tool.ty` section or `ty.toml`.
    /// 1. The closest `pyproject.toml`.
    /// 1. Fallback to use `path` as the root and use the default settings.
    pub fn discover(path: &SystemPath, system: &dyn System) -> Result<Self, ProjectMetadataError> {
        tracing::debug!("Searching for a project in '{path}'");

        if !system.is_directory(path) {
            return Err(ProjectMetadataError::NotADirectory(path.to_path_buf()));
        }

        let use_uv = UseUv::from_system(system);
        let mut closest_project = None;
        for project_root in path.ancestors() {
            let Some(metadata) = Self::discover_in(project_root, system)? else {
                continue;
            };

            if matches!(metadata.configuration_source, ConfigurationSource::Ty) {
                tracing::debug!("Found project at '{}'", project_root);
                return Ok(metadata.with_use_uv(use_uv));
            }

            if closest_project.is_none() {
                closest_project = Some(metadata);
            }
        }

        let metadata = if let Some(closest_project) = closest_project {
            tracing::debug!(
                "Project without `tool.ty` section: '{}'",
                closest_project.root()
            );
            closest_project
        } else {
            tracing::debug!(
                "The ancestor directories contain no `pyproject.toml`. Falling back to a virtual project."
            );
            Self::new(path.file_name().unwrap_or("root"), path.to_path_buf())
        };
        Ok(metadata.with_use_uv(use_uv))
    }

    /// Applies a previously obtained uv workspace without invoking uv or checking [`Self::use_uv`].
    ///
    /// An explicit ty configuration keeps its root; otherwise uv's workspace root supplies
    /// the project configuration. Already applied option layers are preserved.
    /// If loading the workspace configuration fails, this project is left unchanged.
    pub fn apply_uv_workspace(
        &mut self,
        system: &dyn System,
        workspace: UvWorkspace,
    ) -> Result<(), ProjectMetadataError> {
        if !matches!(
            self.configuration_source,
            ConfigurationSource::Ty | ConfigurationSource::ConfigFile(_)
        ) && let Some(uv_metadata) = &workspace.metadata
            && uv_metadata.workspace_root() != self.root()
        {
            let workspace_root = uv_metadata.workspace_root();
            let metadata = Self::discover_in(workspace_root, system)?.unwrap_or_else(|| {
                Self::new(
                    workspace_root.file_name().unwrap_or("root"),
                    workspace_root.to_path_buf(),
                )
            });
            tracing::debug!("Using uv workspace at '{}'", metadata.root());
            *self = metadata.with_applied_options_from(self);
        }

        self.uv_workspace_options = workspace.metadata.as_ref().map(|metadata| {
            Box::new(Options {
                environment: Some(EnvironmentOptions {
                    python: metadata
                        .environment()
                        .map(|path| RelativePathBuf::new(path, ValueSource::UvMetadata)),
                    ..EnvironmentOptions::default()
                }),
                ..Options::default()
            })
        });
        self.uv_workspace = workspace;
        Ok(())
    }

    fn discover_in(
        project_root: &SystemPath,
        system: &dyn System,
    ) -> Result<Option<ProjectMetadata>, ProjectMetadataError> {
        let pyproject_path = project_root.join("pyproject.toml");

        let pyproject = if let Ok(pyproject_str) = system.read_to_string(&pyproject_path) {
            match PyProject::from_toml_str(
                &pyproject_str,
                ValueSource::File(Arc::new(pyproject_path.clone())),
            ) {
                Ok(pyproject) => Some(pyproject),
                Err(error) => {
                    return Err(ProjectMetadataError::InvalidPyProject {
                        path: pyproject_path,
                        source: Box::new(error),
                    });
                }
            }
        } else {
            None
        };

        // A `ty.toml` takes precedence over a `pyproject.toml`.
        let ty_toml_path = project_root.join("ty.toml");
        if let Ok(ty_str) = system.read_to_string(&ty_toml_path) {
            let options = match Options::from_toml_str(
                &ty_str,
                ValueSource::File(Arc::new(ty_toml_path.clone())),
            ) {
                Ok(options) => options,
                Err(error) => {
                    return Err(ProjectMetadataError::InvalidTyToml {
                        path: ty_toml_path,
                        source: Box::new(error),
                    });
                }
            };

            if pyproject
                .as_ref()
                .is_some_and(|project| project.ty().is_some())
            {
                // TODO: Consider using a diagnostic here
                tracing::warn!(
                    "Ignoring the `tool.ty` section in `{pyproject_path}` because `{ty_toml_path}` takes precedence."
                );
            }

            let mut metadata = ProjectMetadata::from_options(
                options,
                project_root.to_path_buf(),
                pyproject
                    .as_ref()
                    .and_then(|pyproject| pyproject.project.as_ref()),
                &FallibleStrategy,
            )
            .map_err(|source| {
                ProjectMetadataError::InvalidRequiresPythonConstraint {
                    source,
                    path: pyproject_path,
                }
            })?;

            metadata.configuration_source = ConfigurationSource::Ty;
            return Ok(Some(metadata));
        }

        let Some(pyproject) = pyproject else {
            return Ok(None);
        };

        let metadata = ProjectMetadata::from_pyproject(pyproject, project_root.to_path_buf())
            .map_err(
                |source| ProjectMetadataError::InvalidRequiresPythonConstraint {
                    source,
                    path: pyproject_path,
                },
            )?;

        Ok(Some(metadata))
    }

    /// Overrides which uv integrations are enabled for this project.
    #[must_use]
    pub fn with_use_uv(mut self, use_uv: UseUv) -> Self {
        self.use_uv = use_uv;
        self
    }

    /// Rediscovers the project from `path`, while preserving applied options.
    pub(crate) fn rediscover(
        &self,
        system: &dyn System,
        path: &SystemPath,
        workspace: UvWorkspace,
    ) -> Result<Self, ProjectMetadataError> {
        let mut metadata = if let Some(config_file) = self.config_file_override() {
            Self::from_config_file(config_file.to_path_buf(), self.root(), system)?
        } else {
            Self::discover(path, system)?
        };

        metadata.apply_uv_workspace(system, workspace)?;
        Ok(metadata.with_applied_options_from(self))
    }

    fn with_applied_options_from(mut self, previous: &Self) -> Self {
        self.use_uv = previous.use_uv;
        self.override_options.clone_from(&previous.override_options);
        self.fallback_options.clone_from(&previous.fallback_options);
        self.user_configuration
            .clone_from(&previous.user_configuration);
        self
    }

    pub fn root(&self) -> &SystemPath {
        &self.root
    }

    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns which uv integrations are enabled for this project.
    pub const fn use_uv(&self) -> UseUv {
        self.use_uv
    }

    pub(crate) fn options(&self) -> &Options {
        &self.options
    }

    pub(crate) fn override_options(&self) -> Option<&Options> {
        self.override_options.as_deref()
    }

    /// Returns the explicit configuration file that replaces normal project discovery, if any.
    pub(crate) fn config_file_override(&self) -> Option<&SystemPath> {
        match &self.configuration_source {
            ConfigurationSource::ConfigFile(path) => Some(path),
            ConfigurationSource::Default
            | ConfigurationSource::Pyproject
            | ConfigurationSource::Ty => None,
        }
    }

    /// Returns configuration paths outside normal project discovery that should be watched.
    pub(crate) fn extra_configuration_paths(&self) -> impl Iterator<Item = &SystemPath> {
        self.config_file_override().into_iter().chain(
            self.user_configuration
                .as_deref()
                .map(|(path, _)| path.as_path()),
        )
    }

    pub(crate) fn try_add_project_root(&self, db: &dyn Db) {
        // This adds a file root for the project itself. This enables
        // tracking of when changes are made to the files in a project
        // at the directory level. At time of writing (2025-07-17),
        // this is used for caching completions for submodules.
        db.files()
            .try_add_root(db, self.root(), FileRootKind::Project);
    }

    /// Sets the highest-precedence options for this project, replacing any previous overrides.
    pub fn set_override_options(&mut self, options: Options) {
        self.override_options = Some(Box::new(options));
    }

    pub(crate) fn uv_workspace(&self) -> &UvWorkspace {
        &self.uv_workspace
    }

    pub(crate) fn uv_diagnostic(&self, db: &dyn Db) -> Option<Diagnostic> {
        let mut diagnostic = self.uv_workspace.error.clone()?;
        let path = self
            .uv_workspace
            .metadata
            .as_ref()
            .map_or(self.root(), uv::UvMetadata::workspace_root)
            .join("pyproject.toml");
        if let Ok(file) = system_path_to_file(db, &path) {
            let mut annotation = Annotation::primary(Span::from(file));
            annotation.hide_snippet(true);
            diagnostic.annotate(annotation);
        }
        Some(diagnostic)
    }

    pub(crate) fn uv_workspace_metadata(&self) -> Option<&uv::UvMetadata> {
        self.uv_workspace.metadata.as_ref()
    }

    /// Sets the lowest-precedence options for this project, replacing any previous fallbacks.
    pub fn set_fallback_options(&mut self, options: Options) {
        self.fallback_options = Some(Box::new(options));
    }

    /// Returns project or script option layers from highest to lowest precedence.
    ///
    /// `options` is the raw project or script configuration, and `uv_options` is its corresponding
    /// uv metadata layer.
    /// Layers can be merged by passing them to [`Options::combine_with`] in iterator order:
    ///
    /// ```ignore
    /// let mut merged = Options::default();
    /// for layer in metadata.options_in_precedence_order(
    ///     metadata.options(),
    ///     metadata.uv_workspace_options.as_deref(),
    /// ) {
    ///     merged.combine_with(layer.clone());
    /// }
    /// ```
    pub(crate) fn options_in_precedence_order<'a>(
        &'a self,
        options: &'a Options,
        uv_options: Option<&'a Options>,
    ) -> impl Iterator<Item = &'a Options> {
        self.override_options
            .as_deref()
            .into_iter()
            .chain(uv_options)
            .chain(std::iter::once(options))
            .chain(
                self.user_configuration
                    .as_deref()
                    .map(|(_, options)| options),
            )
            .chain(self.fallback_options.as_deref())
    }

    /// Returns the configured environment or interpreter path, without resolving the full merged options.
    pub(crate) fn configured_python_path(&self, system: &dyn System) -> Option<SystemPathBuf> {
        self.options_in_precedence_order(&self.options, self.uv_workspace_options.as_deref())
            .find_map(|options| options.environment.as_ref()?.python.as_ref())
            .map(|path| path.absolute(self.root(), system))
    }

    /// Loads the lower-precedence options from the user-level configuration file.
    pub fn apply_configuration_files(
        &mut self,
        system: &dyn System,
    ) -> Result<(), ConfigurationFileError> {
        self.user_configuration = None;

        if let Some(user) = ConfigurationFile::user(system)? {
            tracing::debug!(
                "Applying user-level configuration loaded from `{path}`.",
                path = user.path()
            );
            self.user_configuration = Some(Box::new((user.path().to_owned(), user.into_options())));
        }

        Ok(())
    }

    /// Returns all option layers merged according to their precedence.
    pub fn to_merged_options(&self) -> MergedOptions<'_> {
        let mut options = Options::default();

        for layer in
            self.options_in_precedence_order(&self.options, self.uv_workspace_options.as_deref())
        {
            options.combine_with(layer.clone());
        }

        MergedOptions {
            metadata: self,
            options,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
#[cfg_attr(test, derive(serde::Serialize))]
enum ConfigurationSource {
    /// No configuration file was loaded.
    Default,
    /// A `pyproject.toml` without a `[tool.ty]` section.
    Pyproject,
    /// A `ty.toml` or a `pyproject.toml` with a `[tool.ty]` section, even if empty.
    Ty,
    /// A file explicitly selected with `--config-file`.
    ConfigFile(SystemPathBuf),
}

/// The merged options for a project and the metadata needed to resolve them.
pub struct MergedOptions<'a> {
    metadata: &'a ProjectMetadata,
    options: Options,
}

impl MergedOptions<'_> {
    /// Returns the merged raw options.
    pub fn options(&self) -> &Options {
        &self.options
    }

    pub fn to_program_settings<Strategy: MisconfigurationStrategy>(
        &self,
        system: &dyn System,
        vendored: &VendoredFileSystem,
        strategy: &Strategy,
    ) -> Result<
        (ProgramSettings, Vec<ProgramSettingsDiagnostic>),
        Strategy::Error<ToProgramSettingsError>,
    > {
        self.options.to_program_settings(
            OptionsContext::Project(self.metadata.root()),
            self.metadata.name(),
            system,
            vendored,
            strategy,
        )
    }

    /// Resolve the configured Python environment. Return `None` if no path was configured.
    pub fn python_environment(
        &self,
        system: &dyn System,
    ) -> anyhow::Result<Option<PythonEnvironment>> {
        self.options
            .python_environment(self.metadata.root(), system)
            .map_err(anyhow::Error::from)
    }

    pub fn to_settings<Strategy: MisconfigurationStrategy>(
        &self,
        db: &dyn Db,
        strategy: &Strategy,
    ) -> Result<(Settings, Vec<OptionDiagnostic>), Strategy::Error<ToSettingsError>> {
        self.options
            .to_settings(db, OptionsContext::Project(self.metadata.root()), strategy)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
#[cfg_attr(test, derive(serde::Serialize))]
struct ProjectName(CompactString);

impl ProjectName {
    fn new(name: impl AsRef<str>) -> Self {
        Self(CompactString::new(name))
    }

    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Error)]
pub enum ProjectMetadataError {
    #[error("project path '{0}' is not a directory")]
    NotADirectory(SystemPathBuf),

    #[error("{path} is not a valid `pyproject.toml`")]
    InvalidPyProject {
        source: Box<PyProjectError>,
        path: SystemPathBuf,
    },

    #[error("{path} is not a valid `ty.toml`")]
    InvalidTyToml {
        source: Box<TyTomlError>,
        path: SystemPathBuf,
    },

    #[error("Invalid `requires-python` version specifier (`{path}`)")]
    InvalidRequiresPythonConstraint {
        source: ResolveRequiresPythonError,
        path: SystemPathBuf,
    },

    #[error("Error loading configuration file at {path}")]
    ConfigurationFileError {
        source: Box<ConfigurationFileError>,
        path: SystemPathBuf,
    },
}

#[cfg(test)]
mod tests {
    //! Tests for project discovery, configuration precedence, and option resolution.

    use std::assert_matches;

    use anyhow::{Context, anyhow};
    use insta::assert_ron_snapshot;
    use ruff_db::Db as _;
    use ruff_db::diagnostic::{Diagnostic, DiagnosticId, Severity};
    use ruff_db::files::File;
    #[cfg(unix)]
    use ruff_db::system::{OsSystem, System, SystemPath, WritableSystem};
    use ruff_db::system::{SystemPathBuf, TestSystem};
    use ruff_db::testing::assert_function_query_was_not_run_by_name;
    use ruff_python_ast::PythonVersion;
    use ruff_ranged_value::ValueSource;
    use ty_python_core::program::UseDefaultStrategy;
    use ty_python_semantic::PythonVersionSource;

    use crate::db::{ProjectDatabase, testing::TestDb};
    use crate::metadata::{
        Options, python_version::SupportedPythonVersion, uv::UvMetadata, value::RelativePathBuf,
    };
    use crate::uv::{DependencyMetadataError, UvWorkspace};
    #[cfg(unix)]
    use crate::watch::{ChangeEvent, CreatedKind, DeletedKind};
    use crate::{Db as _, ProjectMetadata, ProjectMetadataError};

    /// Without a `pyproject.toml` or `ty.toml`, the selected directory is the project root.
    /// The project uses default options.
    #[test]
    fn project_without_pyproject() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files_all([(root.join("foo.py"), ""), (root.join("bar.py"), "")])
            .context("Failed to write files")?;

        let project =
            ProjectMetadata::discover(&root, &system).context("Failed to discover project")?;

        assert_eq!(project.root(), &*root);

        with_escaped_paths(|| {
            assert_ron_snapshot!(&project, @r#"
            ProjectMetadata(
              name: ProjectName("app"),
              root: "/app",
              options: Options(),
              configuration_source: Default,
            )
            "#);
        });

        Ok(())
    }

    /// A `pyproject.toml` supplies the project name and makes its directory the project root.
    /// Starting discovery in a subdirectory uses the same `pyproject.toml`.
    #[test]
    fn project_with_pyproject() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files_all([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "backend"

                    "#,
                ),
                (root.join("db/__init__.py"), ""),
            ])
            .context("Failed to write files")?;

        let project =
            ProjectMetadata::discover(&root, &system).context("Failed to discover project")?;

        assert_eq!(project.root(), &*root);

        with_escaped_paths(|| {
            assert_ron_snapshot!(&project, @r#"
            ProjectMetadata(
              name: ProjectName("backend"),
              root: "/app",
              options: Options(),
              configuration_source: Pyproject,
            )
            "#);
        });

        // Discovery from a subdirectory uses the same `pyproject.toml`.
        let from_src = ProjectMetadata::discover(&root.join("db"), &system)
            .context("Failed to discover project from src sub-directory")?;

        assert_eq!(from_src, project);

        Ok(())
    }

    /// An invalid `pyproject.toml` reports its path and the TOML syntax error.
    #[test]
    fn project_with_invalid_pyproject() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files_all([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "backend"

                    [tool.ty
                    "#,
                ),
                (root.join("db/__init__.py"), ""),
            ])
            .context("Failed to write files")?;

        let Err(error) = ProjectMetadata::discover(&root, &system) else {
            return Err(anyhow!(
                "Expected project discovery to fail because of invalid syntax in the pyproject.toml"
            ));
        };

        assert_error_chain_eq(
            error,
            r#"/app/pyproject.toml is not a valid `pyproject.toml`: TOML parse error at line 5, column 29
  |
5 |                     [tool.ty
  |                             ^
unclosed table, expected `]`
"#,
        );

        Ok(())
    }

    /// When nested `pyproject.toml` files both contain `[tool.ty]`, discovery uses the closest one.
    #[test]
    fn nested_projects_in_sub_project() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files_all([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "project-root"

                    [tool.ty.environment]
                    root = ["src"]
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "nested-project"

                    [tool.ty.environment]
                    root = ["src"]
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let sub_project = ProjectMetadata::discover(&root.join("packages/a"), &system)?;

        with_escaped_paths(|| {
            assert_ron_snapshot!(sub_project, @r#"
            ProjectMetadata(
              name: ProjectName("nested-project"),
              root: "/app/packages/a",
              options: Options(
                environment: Some(EnvironmentOptions(
                  root: Some([
                    "src",
                  ]),
                )),
              ),
              configuration_source: Ty,
            )
            "#);
        });

        Ok(())
    }

    /// Discovery uses `[tool.ty]` from the starting directory's `pyproject.toml`, not a subdirectory's.
    #[test]
    fn nested_projects_in_root_project() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files_all([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "project-root"

                    [tool.ty.environment]
                    root = ["src"]
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "nested-project"

                    [tool.ty.environment]
                    root = ["src"]
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let root = ProjectMetadata::discover(&root, &system)?;

        with_escaped_paths(|| {
            assert_ron_snapshot!(root, @r#"
            ProjectMetadata(
              name: ProjectName("project-root"),
              root: "/app",
              options: Options(
                environment: Some(EnvironmentOptions(
                  root: Some([
                    "src",
                  ]),
                )),
              ),
              configuration_source: Ty,
            )
            "#);
        });

        Ok(())
    }

    /// When neither `pyproject.toml` contains `[tool.ty]`, the closest one determines the project root.
    #[test]
    fn nested_projects_without_ty_sections() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files_all([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "project-root"
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "nested-project"
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let sub_project = ProjectMetadata::discover(&root.join("packages/a"), &system)?;

        with_escaped_paths(|| {
            assert_ron_snapshot!(sub_project, @r#"
            ProjectMetadata(
              name: ProjectName("nested-project"),
              root: "/app/packages/a",
              options: Options(),
              configuration_source: Pyproject,
            )
            "#);
        });

        Ok(())
    }

    /// If a member's `pyproject.toml` has no `[tool.ty]`, the uv workspace becomes the project root.
    #[test]
    fn uv_workspace_precedes_plain_member_pyproject() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");
        let member = root.join("packages/member");

        system.memory_file_system().write_files_all([
            (
                root.join("pyproject.toml"),
                r#"
                [tool.uv.workspace]
                members = ["packages/member"]
                "#,
            ),
            (
                member.join("pyproject.toml"),
                r#"
                [project]
                name = "member"
                version = "0.1.0"
                "#,
            ),
        ])?;

        let mut project = ProjectMetadata::discover(&member, &system)?;
        assert_eq!(project.root(), &*member);

        project.apply_uv_workspace(&system, uv_workspace(&root, &system)?)?;

        assert_eq!(project.root(), &*root);

        Ok(())
    }

    /// Without member-local ty configuration, the supplied uv workspace becomes
    /// the project root even when it is not an ancestor of the member directory.
    #[test]
    fn external_uv_workspace_precedes_plain_member_pyproject() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app/workspace");
        let member = SystemPathBuf::from("/app/external-package");

        system.memory_file_system().write_files_all([
            (
                root.join("pyproject.toml"),
                r#"
                [tool.uv.workspace]
                members = ["../external-package"]

                [tool.ty]
                "#,
            ),
            (
                member.join("pyproject.toml"),
                r#"
                [project]
                name = "external-package"
                version = "0.1.0"
                "#,
            ),
        ])?;

        let mut project = ProjectMetadata::discover(&member, &system)?;
        assert_eq!(project.root(), &*member);

        project.apply_uv_workspace(&system, uv_workspace(&root, &system)?)?;

        assert_eq!(project.root(), &*root);

        Ok(())
    }

    /// An empty `[tool.ty]` in a member's `pyproject.toml` makes that directory the project root,
    /// taking precedence over the uv workspace root.
    #[test]
    fn empty_member_pyproject_ty_configuration_precedes_uv_workspace() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");
        let member = root.join("packages/member");

        system.memory_file_system().write_files_all([
            (
                root.join("pyproject.toml"),
                r#"
                [tool.uv.workspace]
                members = ["packages/member"]
                "#,
            ),
            (
                member.join("pyproject.toml"),
                r#"
                [project]
                name = "member"
                version = "0.1.0"

                [tool.ty]
                "#,
            ),
        ])?;

        let mut project = ProjectMetadata::discover(&member, &system)?;

        project.apply_uv_workspace(&system, uv_workspace(&root, &system)?)?;
        assert_eq!(project.root(), &*member);

        Ok(())
    }

    /// `[tool.ty]` in a member's `pyproject.toml` determines the project root and Python version,
    /// taking precedence over the uv workspace's `pyproject.toml`.
    #[test]
    fn member_ty_configuration_precedes_uv_workspace() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");
        let member = root.join("packages/member");

        system.memory_file_system().write_files_all([
            (
                root.join("pyproject.toml"),
                r#"
                [project]
                name = "workspace-root"
                version = "0.1.0"
                requires-python = ">=3.12"

                [tool.uv.workspace]
                members = ["packages/member"]
                "#,
            ),
            (
                member.join("pyproject.toml"),
                r#"
                [project]
                name = "member"
                version = "0.1.0"

                [tool.ty.environment]
                python-version = "3.10"
                "#,
            ),
        ])?;

        let mut project = ProjectMetadata::discover(&member, &system)?;
        project.apply_uv_workspace(&system, uv_workspace(&root, &system)?)?;

        assert_eq!(project.root(), &*member);
        assert_eq!(
            project
                .to_merged_options()
                .options()
                .environment
                .as_ref()
                .and_then(|environment| environment.python_version.as_deref()),
            Some(&SupportedPythonVersion::Py310)
        );

        Ok(())
    }

    /// An empty member-local `ty.toml` keeps the member directory as the project root,
    /// taking precedence over the enclosing uv workspace root.
    #[test]
    fn member_ty_toml_configuration_precedes_uv_workspace() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");
        let member = root.join("packages/member");

        system.memory_file_system().write_files_all([
            (member.join("ty.toml"), ""),
            (
                root.join("pyproject.toml"),
                r#"
                [tool.uv.workspace]
                members = ["packages/member"]
                "#,
            ),
            (
                member.join("pyproject.toml"),
                r#"
                [project]
                name = "member"
                version = "0.1.0"
                "#,
            ),
        ])?;

        let mut project = ProjectMetadata::discover(&member, &system)?;
        project.apply_uv_workspace(&system, uv_workspace(&root, &system)?)?;

        assert_eq!(project.root(), &*member);

        Ok(())
    }

    /// When uv omits the Python environment, ty reports a dependency-metadata warning by default.
    #[test]
    fn dependency_metadata_warning_is_reported_by_default() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from(if cfg!(windows) { "C:/app" } else { "/app" });
        system.memory_file_system().create_directory_all(&root)?;
        let workspace = uv_workspace(&root, &system)?;
        let mut metadata = ProjectMetadata::new("app", root);
        metadata.apply_uv_workspace(&system, workspace)?;
        let db = TestDb::new(metadata);

        let diagnostics = db.project().check_settings(&db);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), DiagnosticId::UvMetadata);
        assert_eq!(diagnostics[0].severity(), Severity::Warning);
        assert_eq!(
            diagnostics[0].concise_message().to_string(),
            "Failed to load uv dependency metadata: uv did not provide a Python environment"
        );

        Ok(())
    }

    #[test]
    fn dependency_metadata_tracks_environment_changes() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from(if cfg!(windows) { "C:/app" } else { "/app" });
        let uv_environment = root.join("uv-venv");
        let selected_environment = root.join("selected-venv");
        system
            .memory_file_system()
            .create_directory_all(&uv_environment)?;
        let input = serde_json::json!({
            "schema": {"version": "preview"},
            "workspace_root": root,
            "environment": {"root": uv_environment},
        });
        let mut metadata = ProjectMetadata::new("app", root);
        metadata.apply_uv_workspace(
            &system,
            UvWorkspace {
                metadata: Some(UvMetadata::from_metadata(
                    &serde_json::to_vec(&input)?,
                    &system,
                )?),
                error: None,
            },
        )?;
        metadata.set_override_options(Options::from_toml_str(
            &format!("[environment]\npython = '{selected_environment}'"),
            ValueSource::Cli,
        )?);
        let mut db = ProjectDatabase::use_defaults(metadata, system.clone());
        let project = db.project();
        assert_matches!(
            project.dependency_metadata(&db),
            Err(DependencyMetadataError::EnvironmentResolution(_))
        );

        // Creating the selected environment refreshes program settings without changing metadata.
        system.memory_file_system().write_file_all(
            selected_environment.join("pyvenv.cfg"),
            "home = /missing\nversion = 3.13.0\ninclude-system-site-packages = false",
        )?;
        system
            .memory_file_system()
            .create_directory_all(selected_environment.join(if cfg!(windows) {
                "Lib/site-packages"
            } else {
                "lib/python3.13/site-packages"
            }))?;
        let (settings, _) = project
            .metadata(&db)
            .to_merged_options()
            .to_program_settings(&system, db.vendored(), &UseDefaultStrategy)?;
        project.update_program(&mut db, settings);
        assert_matches!(
            project.dependency_metadata(&db),
            Err(DependencyMetadataError::EnvironmentMismatch { selected, .. })
                if selected == &selected_environment
        );

        // Directory availability is tracked independently of the resolved program settings.
        system
            .memory_file_system()
            .remove_directory(&uv_environment)?;
        File::sync_path(&mut db, &uv_environment);
        assert_matches!(
            project.dependency_metadata(&db),
            Err(DependencyMetadataError::InvalidEnvironment { .. })
        );

        system
            .memory_file_system()
            .create_directory_all(&uv_environment)?;
        File::sync_path(&mut db, &uv_environment);
        assert_matches!(
            project.dependency_metadata(&db),
            Err(DependencyMetadataError::EnvironmentMismatch { .. })
        );

        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn dependency_metadata_tracks_environment_symlink() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = SystemPath::from_std_path(temp.path()).context("non-UTF-8 temporary path")?;
        let system = OsSystem::new(root);
        let root = system.canonicalize_path(root)?;
        let environment = root.join("environment");
        let link = root.join(".venv");
        system.create_directory_all(&environment.join("lib/python3.13/site-packages"))?;
        system.write_file(
            &environment.join("pyvenv.cfg"),
            "home = /missing\nversion = 3.13.0\ninclude-system-site-packages = false",
        )?;
        std::os::unix::fs::symlink(&environment, &link)?;
        let input = serde_json::json!({
            "schema": {"version": "preview"},
            "workspace_root": root,
            "environment": {"root": link},
            "members": [{"id": "app", "name": "app", "path": root}],
            "resolution": {"app": {"kind": "package", "name": "app", "dependencies": []}},
        });
        let mut metadata = ProjectMetadata::new("app", root);
        metadata.apply_uv_workspace(
            &system,
            UvWorkspace {
                metadata: Some(UvMetadata::from_metadata(
                    &serde_json::to_vec(&input)?,
                    &system,
                )?),
                error: None,
            },
        )?;
        let mut db = ProjectDatabase::fallible(metadata, system.clone())?;
        let project = db.project();
        assert_matches!(project.dependency_metadata(&db), Ok(Some(_)));

        // The selected prefix is canonical, but uv's original path must remain available too.
        std::fs::remove_file(&link)?;
        File::sync_path(&mut db, &link);
        assert_matches!(
            project.dependency_metadata(&db),
            Err(DependencyMetadataError::InvalidEnvironment { .. })
        );

        std::os::unix::fs::symlink(&environment, &link)?;
        File::sync_path(&mut db, &link);
        assert_matches!(project.dependency_metadata(&db), Ok(Some(_)));

        // Syncing only the target must invalidate the query without refreshing program settings.
        std::fs::remove_dir_all(&environment)?;
        File::sync_path(&mut db, &environment);
        assert_matches!(
            project.dependency_metadata(&db),
            Err(DependencyMetadataError::InvalidEnvironment { .. })
        );

        system.create_directory_all(&environment)?;
        File::sync_path(&mut db, &environment);
        assert_matches!(project.dependency_metadata(&db), Ok(Some(_)));

        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn dependency_metadata_tracks_failed_selected_environment_refresh() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = SystemPath::from_std_path(temp.path()).context("non-UTF-8 temporary path")?;
        let system = OsSystem::new(root);
        let root = system.canonicalize_path(root)?;
        let environment = root.join("environment");
        let uv_link = root.join(".venv");
        let selected_link = root.join("selected");
        system.create_directory_all(&environment.join("lib/python3.13/site-packages"))?;
        system.write_file(
            &environment.join("pyvenv.cfg"),
            "home = /missing\nversion = 3.13.0\ninclude-system-site-packages = false",
        )?;
        std::os::unix::fs::symlink(&environment, &uv_link)?;
        std::os::unix::fs::symlink(&environment, &selected_link)?;
        let input = serde_json::json!({
            "schema": {"version": "preview"},
            "workspace_root": root,
            "environment": {"root": uv_link},
            "members": [{"id": "app", "name": "app", "path": root}],
            "resolution": {"app": {"kind": "package", "name": "app", "dependencies": []}},
        });
        let mut metadata = ProjectMetadata::new("app", root.clone());
        metadata.apply_uv_workspace(
            &system,
            UvWorkspace {
                metadata: Some(UvMetadata::from_metadata(
                    &serde_json::to_vec(&input)?,
                    &system,
                )?),
                error: None,
            },
        )?;
        metadata.set_override_options(Options::from_toml_str(
            &format!("[environment]\npython = '{selected_link}'"),
            ValueSource::Cli,
        )?);
        let mut db = ProjectDatabase::fallible(metadata, system)?;
        let project = db.project();
        assert_matches!(project.dependency_metadata(&db), Ok(Some(_)));
        let search_paths = project.program_settings(&db).search_paths.clone();

        // Repeated failures must retain the checking program and the environment used for watching.
        std::fs::remove_file(&selected_link)?;
        for _ in 0..2 {
            let workspace = project.metadata(&db).uv_workspace().clone();
            project.rediscover(&mut db, &root, workspace)?;
            assert_matches!(
                project.dependency_metadata(&db),
                Err(DependencyMetadataError::EnvironmentResolution(message))
                    if message.contains(selected_link.as_str())
            );
            assert_eq!(project.program_settings(&db).search_paths, search_paths);
            assert_eq!(
                project.program_settings(&db).virtual_environment(),
                Some(environment.as_path()),
            );
        }

        std::os::unix::fs::symlink(&environment, &selected_link)?;
        // A different settings error must clear the stale environment error, too.
        let system = OsSystem::new(&root);
        system.create_directory_all(&root.join("broken-typeshed/stdlib"))?;
        system.write_file(
            &root.join("pyproject.toml"),
            "[tool.ty.environment]\ntypeshed = 'broken-typeshed'\n",
        )?;
        let workspace = project.metadata(&db).uv_workspace().clone();
        project.rediscover(&mut db, &root, workspace)?;
        assert_matches!(
            project
                .metadata(&db)
                .to_merged_options()
                .to_program_settings(db.system(), db.vendored(), &super::FallibleStrategy),
            Err(super::ToProgramSettingsError::SearchPaths(_))
        );
        assert_eq!(project.program_settings(&db).search_paths, search_paths);
        assert_matches!(project.dependency_metadata(&db), Ok(Some(_)));

        std::fs::remove_file(root.join("pyproject.toml"))?;
        let workspace = project.metadata(&db).uv_workspace().clone();
        project.rediscover(&mut db, &root, workspace)?;
        assert_matches!(project.dependency_metadata(&db), Ok(Some(_)));

        std::fs::remove_file(&selected_link)?;
        db.apply_changes(&[ChangeEvent::Deleted {
            path: selected_link.clone(),
            kind: DeletedKind::File,
        }]);
        assert_matches!(
            project.dependency_metadata(&db),
            Err(DependencyMetadataError::EnvironmentResolution(_))
        );

        std::os::unix::fs::symlink(&environment, &selected_link)?;
        db.apply_changes(&[ChangeEvent::Created {
            path: selected_link,
            kind: CreatedKind::Any,
        }]);
        assert_matches!(project.dependency_metadata(&db), Ok(Some(_)));

        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn dependency_metadata_rechecks_uv_symlink_after_program_change() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = SystemPath::from_std_path(temp.path()).context("non-UTF-8 temporary path")?;
        let system = OsSystem::new(root);
        let root = system.canonicalize_path(root)?;
        let old_environment = root.join("old");
        let selected_environment = root.join("selected");
        let uv_link = root.join(".venv");
        system.create_directory_all(&old_environment)?;
        system.create_directory_all(&selected_environment.join("lib/python3.13/site-packages"))?;
        system.write_file(
            &selected_environment.join("pyvenv.cfg"),
            "home = /missing\nversion = 3.13.0\ninclude-system-site-packages = false",
        )?;
        std::os::unix::fs::symlink(&old_environment, &uv_link)?;
        let input = serde_json::json!({
            "schema": {"version": "preview"},
            "workspace_root": root,
            "environment": {"root": uv_link},
            "members": [{"id": "app", "name": "app", "path": root}],
            "resolution": {"app": {"kind": "package", "name": "app", "dependencies": []}},
        });
        let input = serde_json::to_vec(&input)?;
        let mut metadata = ProjectMetadata::new("app", root);
        metadata.apply_uv_workspace(
            &system,
            UvWorkspace {
                metadata: Some(UvMetadata::from_metadata(&input, &system)?),
                error: None,
            },
        )?;
        metadata.set_override_options(Options::from_toml_str(
            &format!("[environment]\npython = '{selected_environment}'"),
            ValueSource::Cli,
        )?);
        let mut db = ProjectDatabase::fallible(metadata, system)?;
        let project = db.project();
        assert_matches!(
            project.dependency_metadata(&db),
            Err(DependencyMetadataError::EnvironmentMismatch { .. })
        );

        std::fs::remove_file(&uv_link)?;
        std::os::unix::fs::symlink(&selected_environment, &uv_link)?;
        // An unrelated program change invalidates the query without refreshing uv metadata.
        let mut settings = project.program_settings(&db).clone();
        settings.python_version.version = PythonVersion::PY312;
        project.update_program(&mut db, settings);
        assert_matches!(project.dependency_metadata(&db), Ok(Some(_)));

        Ok(())
    }

    /// A uv refresh failure is reported without a second warning about missing dependency metadata.
    #[test]
    fn uv_refresh_error_takes_precedence_over_dependency_error() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from(if cfg!(windows) { "C:/app" } else { "/app" });
        system.memory_file_system().create_directory_all(&root)?;
        let mut workspace = uv_workspace(&root, &system)?;
        workspace.error = Some(Diagnostic::new(
            DiagnosticId::UvMetadata,
            Severity::Warning,
            "uv metadata refresh failed",
        ));
        let mut metadata = ProjectMetadata::new("app", root);
        metadata.apply_uv_workspace(&system, workspace)?;
        metadata.set_override_options(Options::from_toml_str(
            "[rules]\nmissing-direct-dependency = 'warn'",
            ValueSource::Cli,
        )?);
        let mut db = TestDb::new(metadata);
        let project = db.project();

        assert!(project.metadata(&db).uv_workspace_metadata().is_some());
        let diagnostics = project.check_settings(&db);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), DiagnosticId::UvMetadata);
        assert_eq!(diagnostics[0].severity(), Severity::Warning);
        assert_eq!(
            diagnostics[0].concise_message().to_string(),
            "uv metadata refresh failed"
        );
        let events = db.take_salsa_events();
        assert_function_query_was_not_run_by_name(
            &db,
            "Project::dependency_metadata_",
            None,
            &events,
        );
        assert_matches!(
            project.dependency_metadata(&db),
            Err(DependencyMetadataError::MissingEnvironment)
        );

        Ok(())
    }

    /// The uv environment overrides settings from `pyproject.toml`, user configuration,
    /// and fallback options.
    /// Explicit overrides take precedence over uv.
    #[test]
    fn uv_workspace_options_precedence() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");
        let environment = root.join("uv-venv");
        let user_config_directory = root.join("config");

        system
            .in_memory()
            .set_user_configuration_directory(Some(user_config_directory.clone()));

        system.memory_file_system().write_files_all([
            (
                root.join("pyproject.toml"),
                r#"
                [tool.ty.environment]
                python = "/project-venv"
                "#,
            ),
            (
                user_config_directory.join("ty/ty.toml"),
                r#"
                [environment]
                python = "/user-venv"
                "#,
            ),
            (environment.join("marker"), ""),
        ])?;

        let metadata = serde_json::json!({
            "schema": {"version": "preview"},
            "workspace_root": root,
            "environment": {
                "root": environment,
            },
        });
        let workspace = UvWorkspace {
            metadata: Some(UvMetadata::from_metadata(
                metadata.to_string().as_bytes(),
                &system,
            )?),
            error: None,
        };
        let mut project = ProjectMetadata::discover(&root, &system)?;
        project.apply_configuration_files(&system)?;
        project.set_fallback_options(Options::from_toml_str(
            r#"
            [environment]
            python = "/editor-venv"
            "#,
            ValueSource::Editor,
        )?);
        project.apply_uv_workspace(&system, workspace)?;

        // uv's environment takes precedence over `pyproject.toml`, user configuration,
        // and fallback options.
        assert_eq!(
            project
                .to_merged_options()
                .options()
                .environment
                .as_ref()
                .and_then(|environment| environment.python.as_ref())
                .map(RelativePathBuf::path),
            Some(environment.as_path())
        );

        // An explicit override takes precedence over uv.
        project.set_override_options(Options::from_toml_str(
            r#"
            [environment]
            python = "/override-venv"
            "#,
            ValueSource::Cli,
        )?);

        assert_eq!(
            project
                .to_merged_options()
                .options()
                .environment
                .as_ref()
                .and_then(|environment| environment.python.as_ref())
                .map(|python| python.path().as_str()),
            Some("/override-venv")
        );

        Ok(())
    }

    /// Without `python-version` or `requires-python`, the target Python version comes from
    /// the selected uv environment.
    #[test]
    fn infers_python_version_from_uv_environment() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");
        let environment = root.join("uv-venv");
        let site_packages = if cfg!(windows) {
            environment.join("Lib/site-packages")
        } else {
            environment.join("lib/python3.13/site-packages")
        };

        system.memory_file_system().write_files_all([
            (
                root.join("pyproject.toml"),
                r#"
                [tool.uv.workspace]
                "#,
            ),
            (
                environment.join("pyvenv.cfg"),
                r#"
                home = /missing
                version_info = 3.13.0
                "#,
            ),
            (site_packages.join("marker"), ""),
        ])?;

        let metadata = serde_json::json!({
            "schema": {"version": "preview"},
            "workspace_root": root,
            "environment": {
                "root": environment,
                "python": {"version": "3.13.0"},
            },
        });
        let workspace = UvWorkspace {
            metadata: Some(UvMetadata::from_metadata(
                metadata.to_string().as_bytes(),
                &system,
            )?),
            error: None,
        };
        let mut project = ProjectMetadata::discover(&root, &system)?;
        project.apply_uv_workspace(&system, workspace)?;
        project.apply_configuration_files(&system)?;

        let db = ProjectDatabase::fallible(project, system)?;

        let python_version = &db.project().program_settings(&db).python_version;
        assert_eq!(python_version.version, PythonVersion::PY313);
        assert_matches!(python_version.source, PythonVersionSource::PyvenvCfgFile(_));

        Ok(())
    }

    /// A `pyproject.toml` containing `[tool.ty]` in an ancestor directory takes precedence over
    /// a closer `pyproject.toml` without `[tool.ty]`.
    #[test]
    fn nested_projects_with_outer_ty_section() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files_all([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "project-root"

                    [tool.ty.environment]
                    python-version = "3.10"
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "nested-project"
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let root = ProjectMetadata::discover(&root.join("packages/a"), &system)?;

        with_escaped_paths(|| {
            assert_ron_snapshot!(root, @r#"
            ProjectMetadata(
              name: ProjectName("project-root"),
              root: "/app",
              options: Options(
                environment: Some(EnvironmentOptions(
                  r#python-version: Some(r#3.10),
                )),
              ),
              configuration_source: Ty,
            )
            "#);
        });

        Ok(())
    }

    /// A `ty.toml` takes precedence over `[tool.ty]` in a `pyproject.toml` in the same directory.
    /// The name and `requires-python` from `[project]` in `pyproject.toml` still apply.
    #[test]
    fn project_with_ty_and_pyproject_toml() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files_all([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "super-app"
                    requires-python = ">=3.12"

                    [tool.ty.environment]
                    root = ["this_option_is_ignored"]
                    "#,
                ),
                (
                    root.join("ty.toml"),
                    r#"
                    [environment]
                    root = ["src"]
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let root = ProjectMetadata::discover(&root, &system)?;

        with_escaped_paths(|| {
            assert_ron_snapshot!(root, @r#"
            ProjectMetadata(
              name: ProjectName("super-app"),
              root: "/app",
              options: Options(
                environment: Some(EnvironmentOptions(
                  root: Some([
                    "src",
                  ]),
                  r#python-version: Some(r#3.12),
                )),
              ),
              configuration_source: Ty,
            )
            "#);
        });

        Ok(())
    }

    /// The `requires-python` lower bound supplies the target version when `python-version` is unset.
    #[test]
    fn requires_python_major_minor() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("pyproject.toml"),
                r#"
                [project]
                requires-python = ">=3.12"
                "#,
            )
            .context("Failed to write file")?;

        let root = ProjectMetadata::discover(&root, &system)?;

        assert_eq!(
            root.options
                .environment
                .unwrap_or_default()
                .python_version
                .as_deref()
                .copied()
                .map(PythonVersion::from),
            Some(PythonVersion::PY312)
        );

        Ok(())
    }

    /// `requires-python = ">=3"` selects the oldest Python 3 version supported by ty.
    #[test]
    fn requires_python_major_only() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("pyproject.toml"),
                r#"
                [project]
                requires-python = ">=3"
                "#,
            )
            .context("Failed to write file")?;

        let root = ProjectMetadata::discover(&root, &system)?;

        assert_eq!(
            root.options
                .environment
                .unwrap_or_default()
                .python_version
                .as_deref()
                .copied()
                .map(PythonVersion::from),
            Some(PythonVersion::PY37)
        );

        Ok(())
    }

    /// `requires-python = ">=3.12.8"` targets Python 3.12; patch versions do not change the target.
    #[test]
    fn requires_python_major_minor_patch() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("pyproject.toml"),
                r#"
                [project]
                requires-python = ">=3.12.8"
                "#,
            )
            .context("Failed to write file")?;

        let root = ProjectMetadata::discover(&root, &system)?;

        assert_eq!(
            root.options
                .environment
                .unwrap_or_default()
                .python_version
                .as_deref()
                .copied()
                .map(PythonVersion::from),
            Some(PythonVersion::PY312)
        );

        Ok(())
    }

    /// A lower bound on a Python 3.13 beta release targets Python 3.13.
    #[test]
    fn requires_python_beta_version() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("pyproject.toml"),
                r#"
                [project]
                requires-python = ">= 3.13.0b0"
                "#,
            )
            .context("Failed to write file")?;

        let root = ProjectMetadata::discover(&root, &system)?;

        assert_eq!(
            root.options
                .environment
                .unwrap_or_default()
                .python_version
                .as_deref()
                .copied()
                .map(PythonVersion::from),
            Some(PythonVersion::PY313)
        );

        Ok(())
    }

    /// A `>3.12` lower bound still targets Python 3.12 because it allows later patch releases.
    #[test]
    fn requires_python_greater_than_major_minor() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("pyproject.toml"),
                r#"
                [project]
                # This is somewhat nonsensical because 3.12.1 > 3.12 is true.
                # That's why simplifying the constraint to >= 3.12 is correct
                requires-python = ">3.12"
                "#,
            )
            .context("Failed to write file")?;

        let root = ProjectMetadata::discover(&root, &system)?;

        assert_eq!(
            root.options
                .environment
                .unwrap_or_default()
                .python_version
                .as_deref()
                .copied()
                .map(PythonVersion::from),
            Some(PythonVersion::PY312)
        );

        Ok(())
    }

    /// `python-version` takes precedence if both `requires-python` and `python-version` are configured.
    #[test]
    fn requires_python_and_python_version() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("pyproject.toml"),
                r#"
                [project]
                requires-python = ">=3.12"

                [tool.ty.environment]
                python-version = "3.10"
                "#,
            )
            .context("Failed to write file")?;

        let root = ProjectMetadata::discover(&root, &system)?;

        assert_eq!(
            root.options
                .environment
                .unwrap_or_default()
                .python_version
                .as_deref()
                .copied()
                .map(PythonVersion::from),
            Some(PythonVersion::PY310)
        );

        Ok(())
    }

    /// A `requires-python` upper bound without a lower bound produces a configuration error.
    #[test]
    fn requires_python_less_than() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("pyproject.toml"),
                r#"
                [project]
                requires-python = "<3.12"
                "#,
            )
            .context("Failed to write file")?;

        let Err(error) = ProjectMetadata::discover(&root, &system) else {
            return Err(anyhow!(
                "Expected project discovery to fail because the `requires-python` doesn't specify a lower bound (it only specifies an upper bound)."
            ));
        };

        assert_error_chain_eq(
            error,
            "Invalid `requires-python` version specifier (`/app/pyproject.toml`): value `<3.12` does not contain a lower bound. Add a lower bound to indicate the minimum compatible Python version (e.g., `>=3.13`) or specify a version in `environment.python-version`.",
        );

        Ok(())
    }

    /// An empty `requires-python` value is an error because it specifies no minimum Python version.
    #[test]
    fn requires_python_no_specifiers() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("pyproject.toml"),
                r#"
                [project]
                requires-python = ""
                "#,
            )
            .context("Failed to write file")?;

        let Err(error) = ProjectMetadata::discover(&root, &system) else {
            return Err(anyhow!(
                "Expected project discovery to fail because the `requires-python` specifiers are empty and don't define a lower bound."
            ));
        };

        assert_error_chain_eq(
            error,
            "Invalid `requires-python` version specifier (`/app/pyproject.toml`): value `` does not contain a lower bound. Add a lower bound to indicate the minimum compatible Python version (e.g., `>=3.13`) or specify a version in `environment.python-version`.",
        );

        Ok(())
    }

    /// An out-of-range major version in `requires-python` produces a configuration error.
    #[test]
    fn requires_python_too_large_major_version() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("pyproject.toml"),
                r#"
                [project]
                requires-python = ">=999.0"
                "#,
            )
            .context("Failed to write file")?;

        let Err(error) = ProjectMetadata::discover(&root, &system) else {
            return Err(anyhow!(
                "Expected project discovery to fail because of the requires-python major version that is larger than 255."
            ));
        };

        assert_error_chain_eq(
            error,
            "Invalid `requires-python` version specifier (`/app/pyproject.toml`): The major version `999` is larger than the maximum supported value 255",
        );

        Ok(())
    }

    /// A Python 2 requirement falls back to ty's oldest supported Python 3 version.
    #[test]
    fn requires_python_old_version_uses_lowest_supported_version() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("pyproject.toml"),
                r#"
                [project]
                requires-python = "==2.7"
                "#,
            )
            .context("Failed to write file")?;

        let root = ProjectMetadata::discover(&root, &system)?;

        assert_eq!(
            root.options
                .environment
                .unwrap_or_default()
                .python_version
                .as_deref()
                .copied()
                .map(PythonVersion::from),
            Some(PythonVersion::PY37)
        );

        Ok(())
    }

    /// A requirement limited to an unsupported future Python version produces a configuration error.
    #[test]
    fn requires_python_unsupported_future_version() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("pyproject.toml"),
                r#"
                [project]
                requires-python = "==44.44"
                "#,
            )
            .context("Failed to write file")?;

        let Err(error) = ProjectMetadata::discover(&root, &system) else {
            return Err(anyhow!(
                "Expected project discovery to fail because `requires-python` does not include a ty-supported version."
            ));
        };

        assert_error_chain_eq(
            error,
            "Invalid `requires-python` version specifier (`/app/pyproject.toml`): value `==44.44` does not include any Python version supported by ty. Adjust `requires-python` to include a supported Python 3 version or specify `environment.python-version` explicitly.",
        );

        Ok(())
    }

    #[track_caller]
    fn assert_error_chain_eq(error: ProjectMetadataError, message: &str) {
        let error = anyhow::Error::new(error);
        assert_eq!(format!("{error:#}").replace('\\', "/"), message);
    }

    fn uv_workspace(root: &SystemPathBuf, system: &TestSystem) -> anyhow::Result<UvWorkspace> {
        let metadata = serde_json::json!({
            "schema": {"version": "preview"},
            "workspace_root": root,
        });

        Ok(UvWorkspace {
            metadata: Some(UvMetadata::from_metadata(
                metadata.to_string().as_bytes(),
                system,
            )?),
            error: None,
        })
    }

    fn with_escaped_paths<R>(f: impl FnOnce() -> R) -> R {
        let mut settings = insta::Settings::clone_current();
        settings.add_dynamic_redaction(".root", |content, _path| {
            content.as_str().unwrap().replace('\\', "/")
        });

        settings.bind(f)
    }
}
