use configuration_file::{ConfigurationFile, ConfigurationFileError};
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_python_ast::name::Name;
use std::sync::Arc;
use thiserror::Error;
use ty_python_semantic::ProgramSettings;

use crate::combine::Combine;
use crate::metadata::pyproject::{Project, PyProject, PyProjectError, ResolveRequiresPythonError};
use crate::metadata::value::ValueSource;
pub use options::Options;
use options::TyTomlError;

mod configuration_file;
pub mod options;
pub mod pyproject;
pub mod settings;
pub mod value;

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct ProjectMetadata {
    pub(super) name: Name,

    pub(super) root: SystemPathBuf,

    /// The raw options
    pub(super) options: Options,

    /// Paths of configurations other than the project's configuration that were combined into [`Self::options`].
    ///
    /// This field stores the paths of the configuration files, mainly for
    /// knowing which files to watch for changes.
    ///
    /// The path ordering doesn't imply precedence.
    #[cfg_attr(test, serde(skip_serializing_if = "Vec::is_empty"))]
    pub(super) extra_configuration_paths: Vec<SystemPathBuf>,
}

impl ProjectMetadata {
    /// Creates a project with the given name and root that uses the default options.
    pub fn new(name: Name, root: SystemPathBuf) -> Self {
        Self {
            name,
            root,
            extra_configuration_paths: Vec::default(),
            options: Options::default(),
        }
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
        )
    }

    /// Loads a project from a set of options with an optional pyproject-project table.
    pub fn from_options(
        mut options: Options,
        root: SystemPathBuf,
        project: Option<&Project>,
    ) -> Result<Self, ResolveRequiresPythonError> {
        let name = project
            .and_then(|project| project.name.as_deref())
            .map(|name| Name::new(&**name))
            .unwrap_or_else(|| Name::new(root.file_name().unwrap_or("root")));

        // If the `options` don't specify a python version but the `project.requires-python` field is set,
        // use that as a lower bound instead.
        if let Some(project) = project {
            if options
                .environment
                .as_ref()
                .is_none_or(|env| env.python_version.is_none())
            {
                if let Some(requires_python) = project.resolve_requires_python_lower_bound()? {
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
            extra_configuration_paths: Vec::new(),
        })
    }

    /// Discovers the closest project at `path` and returns its metadata.
    ///
    /// The algorithm traverses upwards in the `path`'s ancestor chain and uses the following precedence
    /// the resolve the project's root.
    ///
    /// 1. The closest `pyproject.toml` with a `tool.ty` section or `ty.toml`.
    /// 1. The closest `pyproject.toml`.
    /// 1. Fallback to use `path` as the root and use the default settings.
    pub fn discover(
        path: &SystemPath,
        system: &dyn System,
    ) -> Result<ProjectMetadata, ProjectDiscoveryError> {
        tracing::debug!("Searching for a project in '{path}'");

        if !system.is_directory(path) {
            return Err(ProjectDiscoveryError::NotADirectory(path.to_path_buf()));
        }

        let mut closest_project: Option<ProjectMetadata> = None;

        for project_root in path.ancestors() {
            let pyproject_path = project_root.join("pyproject.toml");

            let pyproject = if let Ok(pyproject_str) = system.read_to_string(&pyproject_path) {
                match PyProject::from_toml_str(
                    &pyproject_str,
                    ValueSource::File(Arc::new(pyproject_path.clone())),
                ) {
                    Ok(pyproject) => Some(pyproject),
                    Err(error) => {
                        return Err(ProjectDiscoveryError::InvalidPyProject {
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
                        return Err(ProjectDiscoveryError::InvalidTyToml {
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

                tracing::debug!("Found project at '{}'", project_root);

                let metadata = ProjectMetadata::from_options(
                    options,
                    project_root.to_path_buf(),
                    pyproject
                        .as_ref()
                        .and_then(|pyproject| pyproject.project.as_ref()),
                )
                .map_err(|err| {
                    ProjectDiscoveryError::InvalidRequiresPythonConstraint {
                        source: err,
                        path: pyproject_path,
                    }
                })?;

                return Ok(metadata);
            }

            if let Some(pyproject) = pyproject {
                let has_ty_section = pyproject.ty().is_some();
                let metadata =
                    ProjectMetadata::from_pyproject(pyproject, project_root.to_path_buf())
                        .map_err(
                            |err| ProjectDiscoveryError::InvalidRequiresPythonConstraint {
                                source: err,
                                path: pyproject_path,
                            },
                        )?;

                if has_ty_section {
                    tracing::debug!("Found project at '{}'", project_root);

                    return Ok(metadata);
                }

                // Not a project itself, keep looking for an enclosing project.
                if closest_project.is_none() {
                    closest_project = Some(metadata);
                }
            }
        }

        // No project found, but maybe a pyproject.toml was found.
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

            // Create a project with a default configuration
            Self::new(
                path.file_name().unwrap_or("root").into(),
                path.to_path_buf(),
            )
        };

        Ok(metadata)
    }

    pub fn root(&self) -> &SystemPath {
        &self.root
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    pub fn extra_configuration_paths(&self) -> &[SystemPathBuf] {
        &self.extra_configuration_paths
    }

    pub fn to_program_settings(&self, system: &dyn System) -> ProgramSettings {
        self.options
            .to_program_settings(self.root(), self.name(), system)
    }

    /// Combine the project options with the CLI options where the CLI options take precedence.
    pub fn apply_cli_options(&mut self, options: Options) {
        self.options = options.combine(std::mem::take(&mut self.options));
    }

    /// Applies the options from the configuration files to the project's options.
    ///
    /// This includes:
    ///
    /// * The user-level configuration
    pub fn apply_configuration_files(
        &mut self,
        system: &dyn System,
    ) -> Result<(), ConfigurationFileError> {
        if let Some(user) = ConfigurationFile::user(system)? {
            tracing::debug!(
                "Applying user-level configuration loaded from `{path}`.",
                path = user.path()
            );
            self.apply_configuration_file(user);
        }

        Ok(())
    }

    /// Applies a lower-precedence configuration files to the project's options.
    fn apply_configuration_file(&mut self, options: ConfigurationFile) {
        self.extra_configuration_paths
            .push(options.path().to_owned());
        self.options.combine_with(options.into_options());
    }
}

#[derive(Debug, Error)]
pub enum ProjectDiscoveryError {
    #[error("project path '{0}' is not a directory")]
    NotADirectory(SystemPathBuf),

    #[error("{path} is not a valid `pyproject.toml`: {source}")]
    InvalidPyProject {
        source: Box<PyProjectError>,
        path: SystemPathBuf,
    },

    #[error("{path} is not a valid `ty.toml`: {source}")]
    InvalidTyToml {
        source: Box<TyTomlError>,
        path: SystemPathBuf,
    },

    #[error("Invalid `requires-python` version specifier (`{path}`): {source}")]
    InvalidRequiresPythonConstraint {
        source: ResolveRequiresPythonError,
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

    use crate::{ProjectDiscoveryError, ProjectMetadata};

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
                  name: Name("app"),
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
                  name: Name("backend"),
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

        assert_error_eq(
            &error,
            r#"/app/pyproject.toml is not a valid `pyproject.toml`: TOML parse error at line 5, column 29
  |
5 |                     [tool.ty
  |                             ^
invalid table header
expected `.`, `]`
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
              name: Name("nested-project"),
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
                                name: Name("project-root"),
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
              name: Name("nested-project"),
              root: "/app/packages/a",
              options: Options(),
            )
            "#);
        });

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
              name: Name("project-root"),
              root: "/app",
              options: Options(
                environment: Some(EnvironmentOptions(
                  r#python-version: Some("3.10"),
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
              name: Name("super-app"),
              root: "/app",
              options: Options(
                environment: Some(EnvironmentOptions(
                  r#python-version: Some("3.12"),
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
                .as_deref(),
            Some(&PythonVersion::PY312)
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
                .as_deref(),
            Some(&PythonVersion::from((3, 0)))
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
                .as_deref(),
            Some(&PythonVersion::PY312)
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
                .as_deref(),
            Some(&PythonVersion::PY313)
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
                .as_deref(),
            Some(&PythonVersion::PY312)
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
                .as_deref(),
            Some(&PythonVersion::PY310)
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

        assert_error_eq(
            &error,
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

        assert_error_eq(
            &error,
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

        assert_error_eq(
            &error,
            "Invalid `requires-python` version specifier (`/app/pyproject.toml`): The major version `999` is larger than the maximum supported value 255",
        );

        Ok(())
    }

    #[test]
    fn no_src_root_src_layout() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("src/main.py"),
                r#"
                print("Hello, world!")
                "#,
            )
            .context("Failed to write file")?;

        let metadata = ProjectMetadata::discover(&root, &system)?;
        let settings = metadata
            .options
            .to_program_settings(&root, "my_package", &system);

        assert_eq!(
            settings.search_paths.src_roots,
            vec![root.clone(), root.join("src")]
        );

        Ok(())
    }

    #[test]
    fn no_src_root_package_layout() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("psycopg/psycopg/main.py"),
                r#"
                print("Hello, world!")
                "#,
            )
            .context("Failed to write file")?;

        let metadata = ProjectMetadata::discover(&root, &system)?;
        let settings = metadata
            .options
            .to_program_settings(&root, "psycopg", &system);

        assert_eq!(
            settings.search_paths.src_roots,
            vec![root.clone(), root.join("psycopg")]
        );

        Ok(())
    }

    #[test]
    fn no_src_root_flat_layout() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_file_all(
                root.join("my_package/main.py"),
                r#"
                print("Hello, world!")
                "#,
            )
            .context("Failed to write file")?;

        let metadata = ProjectMetadata::discover(&root, &system)?;
        let settings = metadata
            .options
            .to_program_settings(&root, "my_package", &system);

        assert_eq!(settings.search_paths.src_roots, vec![root]);

        Ok(())
    }

    #[track_caller]
    fn assert_error_eq(error: &ProjectDiscoveryError, message: &str) {
        assert_eq!(error.to_string().replace('\\', "/"), message);
    }

    fn with_escaped_paths<R>(f: impl FnOnce() -> R) -> R {
        let mut settings = insta::Settings::clone_current();
        settings.add_dynamic_redaction(".root", |content, _path| {
            content.as_str().unwrap().replace('\\', "/")
        });

        settings.bind(f)
    }
}
