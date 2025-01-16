use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_python_ast::name::Name;

use crate::project::pyproject::{PyProject, PyProjectError};
use crate::project::settings::Configuration;
use red_knot_python_semantic::ProgramSettings;
use thiserror::Error;

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct ProjectMetadata {
    pub(super) name: Name,

    pub(super) root: SystemPathBuf,

    /// The resolved settings for this project.
    pub(super) configuration: Configuration,
}

impl ProjectMetadata {
    /// Creates a project with the given name and root that uses the default configuration options.
    pub fn new(name: Name, root: SystemPathBuf) -> Self {
        Self {
            name,
            root,
            configuration: Configuration::default(),
        }
    }

    /// Loads a project from a `pyproject.toml` file.
    pub(crate) fn from_pyproject(
        pyproject: PyProject,
        root: SystemPathBuf,
        base_configuration: Option<&Configuration>,
    ) -> Self {
        let name = pyproject.project.and_then(|project| project.name);
        let name = name
            .map(|name| Name::new(&*name))
            .unwrap_or_else(|| Name::new(root.file_name().unwrap_or("root")));

        // TODO: load configuration from pyrpoject.toml
        let mut configuration = Configuration::default();

        if let Some(base_configuration) = base_configuration {
            configuration.extend(base_configuration.clone());
        }

        Self {
            name,
            root,
            configuration,
        }
    }

    /// Discovers the closest project at `path` and returns its metadata.
    ///
    /// The algorithm traverses upwards in the `path`'s ancestor chain and uses the following precedence
    /// the resolve the project's root.
    ///
    /// 1. The closest `pyproject.toml` with a `tool.knot` section.
    /// 1. The closest `pyproject.toml`.
    /// 1. Fallback to use `path` as the root and use the default settings.
    pub fn discover(
        path: &SystemPath,
        system: &dyn System,
        base_configuration: Option<&Configuration>,
    ) -> Result<ProjectMetadata, ProjectDiscoveryError> {
        tracing::debug!("Searching for a project in '{path}'");

        if !system.is_directory(path) {
            return Err(ProjectDiscoveryError::NotADirectory(path.to_path_buf()));
        }

        let mut closest_project: Option<ProjectMetadata> = None;

        for ancestor in path.ancestors() {
            let pyproject_path = ancestor.join("pyproject.toml");
            if let Ok(pyproject_str) = system.read_to_string(&pyproject_path) {
                let pyproject = PyProject::from_str(&pyproject_str).map_err(|error| {
                    ProjectDiscoveryError::InvalidPyProject {
                        path: pyproject_path,
                        source: Box::new(error),
                    }
                })?;

                let has_knot_section = pyproject.knot().is_some();
                let metadata = ProjectMetadata::from_pyproject(
                    pyproject,
                    ancestor.to_path_buf(),
                    base_configuration,
                );

                if has_knot_section {
                    let project_root = ancestor;
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

            // Create a package with a default configuration
            Self {
                name: path.file_name().unwrap_or("root").into(),
                root: path.to_path_buf(),
                // TODO create the configuration from the pyproject toml
                configuration: base_configuration.cloned().unwrap_or_default(),
            }
        };

        Ok(metadata)
    }

    pub fn root(&self) -> &SystemPath {
        &self.root
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    pub fn to_program_settings(&self) -> ProgramSettings {
        self.configuration.to_program_settings(self.root())
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

        let project = ProjectMetadata::discover(&root, &system, None)
            .context("Failed to discover project")?;

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

        let project = ProjectMetadata::discover(&root, &system, None)
            .context("Failed to discover project")?;

        assert_eq!(project.root(), &*root);
        snapshot_project!(project);

        // Discovering the same package from a subdirectory should give the same result
        let from_src = ProjectMetadata::discover(&root.join("db"), &system, None)
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

        let Err(error) = ProjectMetadata::discover(&root, &system, None) else {
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

                    [tool.knot]
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "nested-project"

                    [tool.knot]
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let sub_project = ProjectMetadata::discover(&root.join("packages/a"), &system, None)?;

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

                    [tool.knot]
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "nested-project"

                    [tool.knot]
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let root = ProjectMetadata::discover(&root, &system, None)?;

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

        let sub_project = ProjectMetadata::discover(&root.join("packages/a"), &system, None)?;

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

                    [tool.knot]
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

        let root = ProjectMetadata::discover(&root.join("packages/a"), &system, None)?;

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
