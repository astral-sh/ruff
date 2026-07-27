use compact_str::CompactString;
use configuration_file::{ConfigurationFile, ConfigurationFileError};
use ruff_db::files::FileRootKind;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_ranged_value::ValueSource;
use std::sync::Arc;
use thiserror::Error;
use ty_combine::Combine;
use ty_python_core::program::{FallibleStrategy, MisconfigurationStrategy, ProgramSettings};
use ty_static::EnvVars;

use crate::Db;
use crate::metadata::options::{
    EnvironmentOptions, OptionDiagnostic, ProgramSettingsDiagnostic, ToSettingsError,
};
use crate::metadata::pyproject::{Project, PyProject, PyProjectError, ResolveRequiresPythonError};
use crate::metadata::settings::Settings;
use crate::metadata::value::RelativePathBuf;
pub use options::Options;
use options::TyTomlError;

mod configuration_file;
pub mod options;
pub mod pyproject;
pub mod python_version;
pub(crate) mod script;
pub mod settings;
mod uv;
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
    pub(super) options: Options,

    /// The Python version and interpreter path derived from uv workspace metadata.
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

    /// The explicit configuration file that replaces normal project discovery.
    ///
    /// Can be specified using `--config-file <path>`. When `Some`, [`Self::options`] were loaded from this file
    /// instead of from the project's `pyproject.toml` or `ty.toml` file.
    #[cfg_attr(test, serde(skip_serializing_if = "Option::is_none"))]
    config_file_override: Option<SystemPathBuf>,

    #[cfg_attr(test, serde(skip))]
    uv_workspace: Option<uv::UvWorkspace>,
}

impl ProjectMetadata {
    /// Creates a project with the given name and root that uses the default options.
    pub fn new(name: impl AsRef<str>, root: SystemPathBuf) -> Self {
        Self {
            name: ProjectName::new(name),
            root,
            options: Options::default(),
            uv_workspace_options: None,
            override_options: None,
            user_configuration: None,
            fallback_options: None,
            config_file_override: None,
            uv_workspace: None,
        }
    }

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
            uv_workspace_options: None,
            override_options: None,
            user_configuration: None,
            fallback_options: None,
            config_file_override: Some(path),
            uv_workspace: None,
        })
    }

    /// Loads a project from a `pyproject.toml` file.
    pub(crate) fn from_pyproject(
        pyproject: PyProject,
        root: SystemPathBuf,
    ) -> Result<Self, ResolveRequiresPythonError> {
        Self::from_options(
            pyproject.tool.and_then(|tool| tool.ty).unwrap_or_default(),
            root,
            pyproject.project.as_ref(),
            &FallibleStrategy,
        )
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

        // If the `options` don't specify a python version but the `project.requires-python` field is set,
        // use that as a lower bound instead.
        if let Some(project) = project {
            if options
                .environment
                .as_ref()
                .is_none_or(|env| env.python_version.is_none())
            {
                let requires_python = strategy.fallback_opt(
                    project.resolve_requires_python_lower_bound(),
                    |err| {
                        tracing::debug!("skipping invalid requires_python lower bound: {err}");
                    },
                )?;
                if let Some(requires_python) = requires_python.flatten() {
                    let mut environment = options.environment.unwrap_or_default();
                    environment.python_version = Some(requires_python);
                    options.environment = Some(environment);
                }
            }
        }

        Ok(Self {
            name,
            root,
            options,
            uv_workspace_options: None,
            override_options: None,
            user_configuration: None,
            fallback_options: None,
            config_file_override: None,
            uv_workspace: None,
        })
    }

    /// Discovers the closest project at `path` and returns its metadata.
    ///
    /// The algorithm traverses upwards in the `path`'s ancestor chain and uses the following precedence
    /// to resolve the project's root.
    ///
    /// 1. The closest `pyproject.toml` with a `tool.ty` section or `ty.toml`.
    /// 1. The uv workspace root, if uv integration is enabled.
    /// 1. The closest `pyproject.toml`.
    /// 1. Fallback to use `path` as the root and use the default settings.
    pub fn discover(
        path: &SystemPath,
        system: &dyn System,
    ) -> Result<ProjectMetadata, ProjectMetadataError> {
        let uv_workspace = if matches!(system.env_var(EnvVars::TY_UV).as_deref(), Ok("1" | "true"))
        {
            match uv::UvWorkspace::discover(path, system) {
                Ok(workspace) => Some(workspace),
                Err(error) => {
                    tracing::warn!("{error}");
                    None
                }
            }
        } else {
            None
        };

        Self::discover_with_uv_workspace(path, system, uv_workspace)
    }

    /// Discovers the closest project without considering uv workspace metadata.
    pub fn discover_without_uv(
        path: &SystemPath,
        system: &dyn System,
    ) -> Result<ProjectMetadata, ProjectMetadataError> {
        Self::discover_with_uv_workspace(path, system, None)
    }

    fn discover_with_uv_workspace(
        path: &SystemPath,
        system: &dyn System,
        uv_workspace: Option<uv::UvWorkspace>,
    ) -> Result<ProjectMetadata, ProjectMetadataError> {
        tracing::debug!("Searching for a project in '{path}'");

        if !system.is_directory(path) {
            return Err(ProjectMetadataError::NotADirectory(path.to_path_buf()));
        }

        let mut closest_project: Option<ProjectMetadata> = None;
        let mut uv_project: Option<ProjectMetadata> = None;
        let uv_workspace_root = uv_workspace.as_ref().map(uv::UvWorkspace::root);

        for project_root in path.ancestors() {
            let is_uv_workspace_root = uv_workspace_root == Some(project_root);
            let Some((metadata, has_ty_configuration)) = Self::discover_in(project_root, system)?
            else {
                if is_uv_workspace_root {
                    uv_project = Some(Self::new(
                        project_root.file_name().unwrap_or("root"),
                        project_root.to_path_buf(),
                    ));
                }
                continue;
            };

            if has_ty_configuration {
                tracing::debug!("Found project at '{}'", project_root);
                return Ok(metadata.with_uv_workspace(uv_workspace));
            }

            if is_uv_workspace_root {
                uv_project = Some(metadata);
            } else if closest_project.is_none() {
                closest_project = Some(metadata);
            }
        }

        // Workspace members can live outside the workspace directory, so their ancestor chain may
        // never include the workspace root.
        if let Some(workspace_root) = uv_workspace_root
            && !path.starts_with(workspace_root)
        {
            let metadata = Self::discover_in(workspace_root, system)?
                .map(|(metadata, _)| metadata)
                .unwrap_or_else(|| {
                    Self::new(
                        workspace_root.file_name().unwrap_or("root"),
                        workspace_root.to_path_buf(),
                    )
                });
            uv_project = Some(metadata);
        }

        let metadata = if let Some(uv_project) = uv_project {
            tracing::debug!("Using uv workspace at '{}'", uv_project.root());

            uv_project
        } else if let Some(closest_project) = closest_project {
            tracing::debug!(
                "Project without `tool.ty` section: '{}'",
                closest_project.root()
            );

            closest_project
        } else {
            tracing::debug!(
                "The ancestor directories contain no `pyproject.toml`. Falling back to a virtual project."
            );

            // Create a project with a default configuration
            Self::new(path.file_name().unwrap_or("root"), path.to_path_buf())
        };

        Ok(metadata.with_uv_workspace(uv_workspace))
    }

    fn discover_in(
        project_root: &SystemPath,
        system: &dyn System,
    ) -> Result<Option<(ProjectMetadata, bool)>, ProjectMetadataError> {
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

            let metadata = ProjectMetadata::from_options(
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

            return Ok(Some((metadata, true)));
        }

        let Some(pyproject) = pyproject else {
            return Ok(None);
        };

        let has_ty_configuration = pyproject.ty().is_some();
        let metadata = ProjectMetadata::from_pyproject(pyproject, project_root.to_path_buf())
            .map_err(
                |source| ProjectMetadataError::InvalidRequiresPythonConstraint {
                    source,
                    path: pyproject_path,
                },
            )?;

        Ok(Some((metadata, has_ty_configuration)))
    }

    #[must_use]
    fn with_uv_workspace(mut self, uv_workspace: Option<uv::UvWorkspace>) -> Self {
        self.uv_workspace = uv_workspace;
        self
    }

    /// Rediscovers the project, while preserving applied options.
    pub(crate) fn rediscover(&self, system: &dyn System) -> Result<Self, ProjectMetadataError> {
        let mut metadata = if let Some(config_file) = self.config_file_override() {
            Self::from_config_file(config_file.to_path_buf(), self.root(), system)?
        } else {
            // The active project root may have been deleted. Start rediscovery from the closest
            // existing ancestor so ty can fall back to an enclosing project.
            let rediscovery_path = self
                .root()
                .ancestors()
                .find(|path| system.is_directory(path))
                .unwrap_or_else(|| self.root());
            Self::discover(rediscovery_path, system)?
        };

        metadata.override_options.clone_from(&self.override_options);
        metadata.fallback_options.clone_from(&self.fallback_options);

        Ok(metadata)
    }

    pub fn root(&self) -> &SystemPath {
        &self.root
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Returns the explicit configuration file that replaces normal project discovery, if any.
    pub(crate) fn config_file_override(&self) -> Option<&SystemPath> {
        self.config_file_override.as_deref()
    }

    /// Returns configuration paths outside normal project discovery that should be watched.
    pub fn extra_configuration_paths(&self) -> impl Iterator<Item = &SystemPath> {
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

    /// Applies higher-precedence options to this project.
    ///
    /// Options applied later take precedence over options applied earlier.
    pub fn apply_override_options(&mut self, options: Options) {
        if let Some(existing) = self.override_options.as_mut() {
            let previous = std::mem::replace(existing.as_mut(), options);
            existing.combine_with(previous);
        } else {
            self.override_options = Some(Box::new(options));
        }
    }

    pub fn has_uv_workspace(&self) -> bool {
        self.uv_workspace.is_some()
    }

    /// Applies lower-precedence options to this project.
    ///
    /// Options applied later take precedence over options applied earlier, but all fallback options
    /// have lower precedence than the raw, uv workspace, and user-level options.
    pub fn apply_fallback_options(&mut self, options: Options) {
        if let Some(existing) = self.fallback_options.as_mut() {
            let previous = std::mem::replace(existing.as_mut(), options);
            existing.combine_with(previous);
        } else {
            self.fallback_options = Some(Box::new(options));
        }
    }

    /// Returns the project's option layers from highest to lowest precedence.
    ///
    /// `options` is used as the raw base layer between the uv workspace and user-level options.
    /// Layers can be merged by passing them to [`Options::combine_with`] in iterator order:
    ///
    /// ```ignore
    /// let mut merged = Options::default();
    /// for layer in metadata.options_in_precedence_order(metadata.options()) {
    ///     merged.combine_with(layer.clone());
    /// }
    /// ```
    pub(crate) fn options_in_precedence_order<'a>(
        &'a self,
        options: &'a Options,
    ) -> impl Iterator<Item = &'a Options> {
        self.override_options
            .as_deref()
            .into_iter()
            .chain(self.uv_workspace_options.as_deref())
            .chain(std::iter::once(options))
            .chain(
                self.user_configuration
                    .as_deref()
                    .map(|(_, options)| options),
            )
            .chain(self.fallback_options.as_deref())
    }

    /// Loads the lower-precedence options from configuration files.
    ///
    /// This includes:
    ///
    /// * The uv workspace configuration
    /// * The user-level configuration
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

        self.uv_workspace_options = self.uv_workspace.as_ref().map(|uv_workspace| {
            Box::new(Options {
                environment: Some(EnvironmentOptions {
                    python_version: uv_workspace.python_version().cloned(),
                    python: uv_workspace
                        .environment()
                        .map(|path| RelativePathBuf::new(path, ValueSource::UvWorkspace)),
                    ..EnvironmentOptions::default()
                }),
                ..Options::default()
            })
        });

        Ok(())
    }

    /// Returns all option layers merged according to their precedence.
    pub fn to_merged_options(&self) -> MergedOptions<'_> {
        let mut options = Options::default();

        for layer in self.options_in_precedence_order(&self.options) {
            options.combine_with(layer.clone());
        }

        MergedOptions {
            metadata: self,
            options,
        }
    }
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
    ) -> Result<(ProgramSettings, Vec<ProgramSettingsDiagnostic>), Strategy::Error<anyhow::Error>>
    {
        self.options.to_program_settings(
            self.metadata.root(),
            self.metadata.name(),
            system,
            vendored,
            strategy,
        )
    }

    pub fn to_settings<Strategy: MisconfigurationStrategy>(
        &self,
        db: &dyn Db,
        strategy: &Strategy,
    ) -> Result<(Settings, Vec<OptionDiagnostic>), Strategy::Error<ToSettingsError>> {
        self.options.to_settings(db, self.metadata.root(), strategy)
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
    //! Integration tests for project discovery

    use anyhow::{Context, anyhow};
    use insta::assert_ron_snapshot;
    use ruff_db::system::{SystemPathBuf, TestSystem};
    use ruff_python_ast::PythonVersion;
    use ruff_ranged_value::ValueSource;
    use ty_static::EnvVars;

    use crate::metadata::{Options, uv::UvWorkspace, value::RelativePathBuf};
    use crate::{ProjectMetadata, ProjectMetadataError};

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
            )
            "#);
        });

        Ok(())
    }

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
            )
            "#);
        });

        // Discovering the same package from a subdirectory should give the same result
        let from_src = ProjectMetadata::discover(&root.join("db"), &system)
            .context("Failed to discover project from src sub-directory")?;

        assert_eq!(from_src, project);

        Ok(())
    }

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

                    [tool.ty.src]
                    root = "src"
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "nested-project"

                    [tool.ty.src]
                    root = "src"
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
                src: Some(SrcOptions(
                  root: Some("src"),
                )),
              ),
            )
            "#);
        });

        Ok(())
    }

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

                    [tool.ty.src]
                    root = "src"
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "nested-project"

                    [tool.ty.src]
                    root = "src"
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
                src: Some(SrcOptions(
                  root: Some("src"),
                )),
              ),
            )
            "#);
        });

        Ok(())
    }

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
            )
            "#);
        });

        Ok(())
    }

    #[test]
    fn uv_workspace_precedes_plain_member_pyproject() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");
        let member = root.join("packages/member");

        system.memory_file_system().write_files_all([
            (root.join("pyproject.toml"), "[tool.uv.workspace]"),
            (
                member.join("pyproject.toml"),
                r#"
                [project]
                name = "member"
                "#,
            ),
        ])?;

        let uv_workspace = uv_workspace(&root, &system)?;
        let project =
            ProjectMetadata::discover_with_uv_workspace(&member, &system, Some(uv_workspace))?;

        assert_eq!(project.root(), &*root);

        Ok(())
    }

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

                [tool.ty.rules]
                invalid-assignment = "ignore"
                "#,
            ),
            (
                member.join("pyproject.toml"),
                r#"
                [project]
                name = "external-package"
                "#,
            ),
        ])?;

        let uv_workspace = uv_workspace(&root, &system)?;
        let project =
            ProjectMetadata::discover_with_uv_workspace(&member, &system, Some(uv_workspace))?;

        assert_eq!(project.root(), &*root);

        Ok(())
    }

    #[test]
    fn uv_workspace_discovery_is_system_independent() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");
        let member = root.join("packages/member");

        system.set_env_var(EnvVars::TY_UV, "1");
        system.set_env_var(EnvVars::UV, "uv");
        system
            .memory_file_system()
            .write_file_all(member.join("pyproject.toml"), "[project]\nname = 'member'")?;

        let project = ProjectMetadata::discover(&member, &system)?;

        assert_eq!(project.root(), &*member);

        Ok(())
    }

    #[test]
    fn member_ty_configuration_selects_project_root() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");
        let member = root.join("packages/member");

        system.memory_file_system().write_files_all([
            (root.join("uv.toml"), ""),
            (
                member.join("pyproject.toml"),
                r#"
                [project]
                name = "member"

                [tool.ty.environment]
                python-version = "3.10"
                "#,
            ),
        ])?;

        let uv_workspace = uv_workspace(&root, &system)?;
        let mut project =
            ProjectMetadata::discover_with_uv_workspace(&member, &system, Some(uv_workspace))?;
        project.apply_configuration_files(&system)?;

        assert_eq!(project.root(), &*member);
        assert_eq!(
            project
                .to_merged_options()
                .options()
                .environment
                .as_ref()
                .and_then(|environment| environment.python_version.as_deref())
                .copied()
                .map(PythonVersion::from),
            Some(PythonVersion::PY310)
        );

        Ok(())
    }

    #[test]
    fn outer_ty_configuration_precedes_uv_workspace() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");
        let workspace = root.join("workspace");
        let member = workspace.join("packages/member");

        system.memory_file_system().write_files_all([
            (
                root.join("ty.toml"),
                r#"
                [environment]
                python-version = "3.10"
                "#,
            ),
            (workspace.join("pyproject.toml"), "[tool.uv.workspace]"),
            (
                member.join("pyproject.toml"),
                r#"
                [project]
                name = "member"
                "#,
            ),
        ])?;

        let uv_workspace = uv_workspace(&workspace, &system)?;
        let project =
            ProjectMetadata::discover_with_uv_workspace(&member, &system, Some(uv_workspace))?;

        assert_eq!(project.root(), &*root);

        Ok(())
    }

    #[test]
    fn applies_uv_workspace_environment() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");
        let member = root.join("packages/member");
        let environment = root.join("uv-venv");

        system.memory_file_system().write_files_all([
            (
                root.join("pyproject.toml"),
                r#"
                [tool.uv.workspace]

                [tool.ty.environment]
                python = "/project-venv"
                python-version = "3.10"
                "#,
            ),
            (member.join("pyproject.toml"), "[project]\nname = 'member'"),
            (environment.join("marker"), ""),
        ])?;

        let metadata = serde_json::json!({
            "workspace_root": root,
            "environment": {
                "root": environment,
                "python": {
                    "version": "3.13.5",
                },
            },
        });
        let uv_workspace = UvWorkspace::from_metadata(metadata.to_string().as_bytes(), &system)?;
        let mut project =
            ProjectMetadata::discover_with_uv_workspace(&member, &system, Some(uv_workspace))?;
        project.apply_fallback_options(Options::from_toml_str(
            r#"
            [environment]
            python = "/editor-venv"
            python-version = "3.10"
            "#,
            ValueSource::Editor,
        )?);
        project.apply_configuration_files(&system)?;

        let merged_options = project.to_merged_options();
        let project_environment = merged_options.options().environment.as_ref();
        assert_eq!(
            project_environment
                .and_then(|environment| environment.python_version.as_deref())
                .copied()
                .map(PythonVersion::from),
            Some(PythonVersion::PY313)
        );
        assert_eq!(
            project_environment
                .and_then(|environment| environment.python.as_ref())
                .map(RelativePathBuf::path),
            Some(environment.as_path())
        );
        assert!(matches!(
            project_environment
                .and_then(|environment| environment.python.as_ref())
                .map(RelativePathBuf::source),
            Some(ValueSource::UvWorkspace)
        ));
        assert!(matches!(
            project_environment
                .and_then(|environment| environment.python_version.as_ref())
                .map(ruff_ranged_value::RangedValue::source),
            Some(ValueSource::UvWorkspace)
        ));

        let user_config_directory = root.join("config");
        system
            .in_memory()
            .set_user_configuration_directory(Some(user_config_directory.clone()));
        system.memory_file_system().write_file_all(
            user_config_directory.join("ty/ty.toml"),
            r#"
            [environment]
            python = "/user-venv"
            python-version = "3.12"
            "#,
        )?;
        project.apply_configuration_files(&system)?;

        let merged_options = project.to_merged_options();
        let project_environment = merged_options.options().environment.as_ref();
        assert_eq!(
            project_environment
                .and_then(|environment| environment.python_version.as_deref())
                .copied()
                .map(PythonVersion::from),
            Some(PythonVersion::PY313)
        );
        assert_eq!(
            project_environment
                .and_then(|environment| environment.python.as_ref())
                .map(|python| python.path().as_str()),
            Some(environment.as_str())
        );

        Ok(())
    }

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
            )
            "#);
        });

        Ok(())
    }

    /// A `ty.toml` takes precedence over any `pyproject.toml`.
    ///
    /// However, the `pyproject.toml` is still loaded to get the project name and, in the future,
    /// the requires-python constraint.
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

                    [tool.ty.src]
                    root = "this_option_is_ignored"
                    "#,
                ),
                (
                    root.join("ty.toml"),
                    r#"
                    [src]
                    root = "src"
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
                  r#python-version: Some(r#3.12),
                )),
                src: Some(SrcOptions(
                  root: Some("src"),
                )),
              ),
            )
            "#);
        });

        Ok(())
    }
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

    /// A `requires-python` constraint with major, minor and patch can be simplified
    /// to major and minor (e.g. 3.12.1 -> 3.12).
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
            "workspace_root": root,
        });

        Ok(UvWorkspace::from_metadata(
            metadata.to_string().as_bytes(),
            system,
        )?)
    }

    fn with_escaped_paths<R>(f: impl FnOnce() -> R) -> R {
        let mut settings = insta::Settings::clone_current();
        settings.add_dynamic_redaction(".root", |content, _path| {
            content.as_str().unwrap().replace('\\', "/")
        });

        settings.bind(f)
    }
}
