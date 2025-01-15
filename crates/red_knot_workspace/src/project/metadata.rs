use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_python_ast::name::Name;

use crate::project::combine::Combine;
use crate::project::options::Options;
use crate::project::pyproject::{PyProject, PyProjectError};
use red_knot_python_semantic::ProgramSettings;
use thiserror::Error;

use super::options::KnotTomlError;

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct ProjectMetadata {
    pub(super) name: Name,

    pub(super) root: SystemPathBuf,

    /// The raw options
    pub(super) options: Options,
}

impl ProjectMetadata {
    /// Creates a project with the given name and root that uses the default options.
    pub fn new(name: Name, root: SystemPathBuf) -> Self {
        Self {
            name,
            root,
            options: Options::default(),
        }
    }

    /// Loads a project from a `pyproject.toml` file.
    pub(crate) fn from_pyproject(pyproject: PyProject, root: SystemPathBuf) -> Self {
        let name = Self::name_from_pyproject(Some(&pyproject), &root);
        let options = pyproject
            .tool
            .and_then(|tool| tool.knot)
            .unwrap_or_default();

        Self {
            name,
            root,
            options,
        }
    }

    fn name_from_pyproject(pyproject: Option<&PyProject>, root: &SystemPath) -> Name {
        let name = pyproject
            .and_then(|pyproject| pyproject.project.as_ref())
            .and_then(|project| project.name.as_ref());
        name.map(|name| Name::new(&**name))
            .unwrap_or_else(|| Name::new(root.file_name().unwrap_or("root")))
    }

    /// Loads a project form a `knot.toml` file.
    pub(crate) fn from_knot_toml(
        options: Options,
        root: SystemPathBuf,
        pyproject: Option<&PyProject>,
    ) -> Self {
        // TODO(https://github.com/astral-sh/ruff/issues/15491): Respect requires-python
        Self {
            name: Self::name_from_pyproject(pyproject, &root),
            root,
            options,
        }
    }

    /// Discovers the closest project at `path` and returns its metadata.
    ///
    /// The algorithm traverses upwards in the `path`'s ancestor chain and uses the following precedence
    /// the resolve the project's root.
    ///
    /// 1. The closest `pyproject.toml` with a `tool.knot` section or `knot.toml`.
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
                match PyProject::from_toml_str(&pyproject_str) {
                    Ok(pyproject) => Some(pyproject),
                    Err(error) => {
                        return Err(ProjectDiscoveryError::InvalidPyProject {
                            path: pyproject_path,
                            source: Box::new(error),
                        })
                    }
                }
            } else {
                None
            };

            // A `knot.toml` takes precedence over a `pyproject.toml`.
            let knot_toml_path = project_root.join("knot.toml");
            if let Ok(knot_str) = system.read_to_string(&knot_toml_path) {
                let options = match Options::from_toml_str(&knot_str) {
                    Ok(options) => options,
                    Err(error) => {
                        return Err(ProjectDiscoveryError::InvalidKnotToml {
                            path: knot_toml_path,
                            source: Box::new(error),
                        })
                    }
                };

                if pyproject
                    .as_ref()
                    .is_some_and(|project| project.knot().is_some())
                {
                    // TODO: Consider using a diagnostic here
                    tracing::warn!("Ignoring the `tool.knot` section in `{pyproject_path}` because `{knot_toml_path}` takes precedence.");
                }

                tracing::debug!("Found project at '{}'", project_root);
                return Ok(ProjectMetadata::from_knot_toml(
                    options,
                    project_root.to_path_buf(),
                    pyproject.as_ref(),
                ));
            }

            if let Some(pyproject) = pyproject {
                let has_knot_section = pyproject.knot().is_some();
                let metadata =
                    ProjectMetadata::from_pyproject(pyproject, project_root.to_path_buf());

                if has_knot_section {
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
                "Project without `tool.knot` section: '{}'",
                closest_project.root()
            );

            closest_project
        } else {
            tracing::debug!("The ancestor directories contain no `pyproject.toml`. Falling back to a virtual project.");

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

    pub fn to_program_settings(&self, system: &dyn System) -> ProgramSettings {
        self.options.to_program_settings(self.root(), system)
    }

    /// Combine the project options with the CLI options where the CLI options take precedence.
    pub fn apply_cli_options(&mut self, options: Options) {
        self.options = options.combine(std::mem::take(&mut self.options));
    }

    /// Combine the project options with the user options where project options take precedence.
    pub fn apply_user_options(&mut self, options: Options) {
        self.options.combine_with(options);
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

    #[error("{path} is not a valid `knot.toml`: {source}")]
    InvalidKnotToml {
        source: Box<KnotTomlError>,
        path: SystemPathBuf,
    },
}

#[cfg(test)]
mod tests {
    //! Integration tests for project discovery

    use crate::snapshot_project;
    use anyhow::{anyhow, Context};
    use insta::assert_ron_snapshot;
    use ruff_db::system::{SystemPathBuf, TestSystem};

    use crate::project::{ProjectDiscoveryError, ProjectMetadata};

    #[test]
    fn project_without_pyproject() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([(root.join("foo.py"), ""), (root.join("bar.py"), "")])
            .context("Failed to write files")?;

        let project =
            ProjectMetadata::discover(&root, &system).context("Failed to discover project")?;

        assert_eq!(project.root(), &*root);

        snapshot_project!(project);

        Ok(())
    }

    #[test]
    fn project_with_pyproject() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
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
        snapshot_project!(project);

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
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "backend"

                    [tool.knot
                    "#,
                ),
                (root.join("db/__init__.py"), ""),
            ])
            .context("Failed to write files")?;

        let Err(error) = ProjectMetadata::discover(&root, &system) else {
            return Err(anyhow!("Expected project discovery to fail because of invalid syntax in the pyproject.toml"));
        };

        assert_error_eq(
            &error,
            r#"/app/pyproject.toml is not a valid `pyproject.toml`: TOML parse error at line 5, column 31
  |
5 |                     [tool.knot
  |                               ^
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
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "project-root"

                    [tool.knot.src]
                    root = "src"
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "nested-project"

                    [tool.knot.src]
                    root = "src"
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let sub_project = ProjectMetadata::discover(&root.join("packages/a"), &system)?;

        snapshot_project!(sub_project);

        Ok(())
    }

    #[test]
    fn nested_projects_in_root_project() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "project-root"

                    [tool.knot.src]
                    root = "src"
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "nested-project"

                    [tool.knot.src]
                    root = "src"
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let root = ProjectMetadata::discover(&root, &system)?;

        snapshot_project!(root);

        Ok(())
    }

    #[test]
    fn nested_projects_without_knot_sections() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
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

        snapshot_project!(sub_project);

        Ok(())
    }

    #[test]
    fn nested_projects_with_outer_knot_section() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "project-root"

                    [tool.knot.environment]
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

        snapshot_project!(root);

        Ok(())
    }

    /// A `knot.toml` takes precedence over any `pyproject.toml`.
    ///
    /// However, the `pyproject.toml` is still loaded to get the project name and, in the future,
    /// the requires-python constraint.
    #[test]
    fn project_with_knot_and_pyproject_toml() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                        [project]
                        name = "super-app"
                        requires-python = ">=3.12"

                        [tool.knot.src]
                        root = "this_option_is_ignored"
                        "#,
                ),
                (
                    root.join("knot.toml"),
                    r#"
                        [src]
                        root = "src"
                        "#,
                ),
            ])
            .context("Failed to write files")?;

        let root = ProjectMetadata::discover(&root, &system)?;

        snapshot_project!(root);

        Ok(())
    }

    #[track_caller]
    fn assert_error_eq(error: &ProjectDiscoveryError, message: &str) {
        assert_eq!(error.to_string().replace('\\', "/"), message);
    }

    /// Snapshots a project but with all paths using unix separators.
    #[macro_export]
    macro_rules! snapshot_project {
    ($project:expr) => {{
        assert_ron_snapshot!($project,{
            ".root" => insta::dynamic_redaction(|content, _content_path| {
                content.as_str().unwrap().replace("\\", "/")
            }),
        });
    }};
}
}
