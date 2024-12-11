use ruff_db::system::{GlobError, System, SystemPath, SystemPathBuf};
use ruff_python_ast::name::Name;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use thiserror::Error;

use crate::workspace::pyproject::{PyProject, PyProjectError, Workspace};
use crate::workspace::settings::{Configuration, WorkspaceSettings};

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct WorkspaceMetadata {
    pub(super) root: SystemPathBuf,

    /// The (first-party) packages in this workspace.
    pub(super) packages: Vec<PackageMetadata>,

    /// The resolved settings for this workspace.
    pub(super) settings: WorkspaceSettings,
}

/// A first-party package in a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct PackageMetadata {
    pub(super) name: Name,

    /// The path to the root directory of the package.
    pub(super) root: SystemPathBuf,

    pub(super) configuration: Configuration,
}

impl WorkspaceMetadata {
    /// Creates a workspace that consists of a single package located at `root`.
    pub fn single_package(name: Name, root: SystemPathBuf) -> Self {
        let package = PackageMetadata {
            name,
            root: root.clone(),
            configuration: Configuration::default(),
        };

        let packages = vec![package];
        let settings = packages[0]
            .configuration
            .to_workspace_settings(&root, &packages);

        Self {
            root,
            packages,
            settings,
        }
    }

    /// Discovers the closest workspace at `path` and returns its metadata.
    ///
    /// 1. Traverse upwards in the `path`'s ancestor chain and find the first `pyproject.toml`.
    /// 1. If the `pyproject.toml` contains no `knot.workspace` table, then keep traversing the `path`'s ancestor
    ///    chain until we find one or reach the root.
    /// 1. If we've found a workspace, then resolve the workspace's members and assert that the closest
    ///    package (the first found package without a `knot.workspace` table is a member. If not, create
    ///    a single package workspace for the closest package.
    /// 1. If there's no `pyrpoject.toml` with a `knot.workspace` table, then create a single-package workspace.
    /// 1. If no ancestor directory contains any `pyproject.toml`, create an ad-hoc workspace for `path`
    ///    that consists of a single package and uses the default settings.
    pub fn discover(
        path: &SystemPath,
        system: &dyn System,
        base_configuration: Option<&Configuration>,
    ) -> Result<WorkspaceMetadata, WorkspaceDiscoveryError> {
        tracing::debug!("Searching for a workspace in '{path}'");

        if !system.is_directory(path) {
            return Err(WorkspaceDiscoveryError::NotADirectory(path.to_path_buf()));
        }

        let mut closest_package: Option<PackageMetadata> = None;

        for ancestor in path.ancestors() {
            let pyproject_path = ancestor.join("pyproject.toml");
            if let Ok(pyproject_str) = system.read_to_string(&pyproject_path) {
                let pyproject = PyProject::from_str(&pyproject_str).map_err(|error| {
                    WorkspaceDiscoveryError::InvalidPyProject {
                        path: pyproject_path,
                        source: Box::new(error),
                    }
                })?;

                let workspace_table = pyproject.workspace().cloned();
                let package = PackageMetadata::from_pyproject(
                    pyproject,
                    ancestor.to_path_buf(),
                    base_configuration,
                );

                if let Some(workspace_table) = workspace_table {
                    let workspace_root = ancestor;
                    tracing::debug!("Found workspace at '{}'", workspace_root);

                    match collect_packages(
                        package,
                        &workspace_table,
                        closest_package,
                        base_configuration,
                        system,
                    )? {
                        CollectedPackagesOrStandalone::Packages(mut packages) => {
                            let mut by_name =
                                FxHashMap::with_capacity_and_hasher(packages.len(), FxBuildHasher);

                            let mut workspace_package = None;

                            for package in &packages {
                                if let Some(conflicting) = by_name.insert(package.name(), package) {
                                    return Err(WorkspaceDiscoveryError::DuplicatePackageNames {
                                        name: package.name().clone(),
                                        first: conflicting.root().to_path_buf(),
                                        second: package.root().to_path_buf(),
                                    });
                                }

                                if package.root() == workspace_root {
                                    workspace_package = Some(package);
                                } else if !package.root().starts_with(workspace_root) {
                                    return Err(WorkspaceDiscoveryError::PackageOutsideWorkspace {
                                        package_name: package.name().clone(),
                                        package_root: package.root().to_path_buf(),
                                        workspace_root: workspace_root.to_path_buf(),
                                    });
                                }
                            }

                            let workspace_package = workspace_package
                                .expect("workspace package to be part of the workspace's packages");

                            let settings = workspace_package
                                .configuration
                                .to_workspace_settings(workspace_root, &packages);

                            packages.sort_unstable_by(|a, b| a.root().cmp(b.root()));

                            return Ok(Self {
                                root: workspace_root.to_path_buf(),
                                packages,
                                settings,
                            });
                        }
                        CollectedPackagesOrStandalone::Standalone(package) => {
                            closest_package = Some(package);
                            break;
                        }
                    }
                }

                // Not a workspace itself, keep looking for an enclosing workspace.
                if closest_package.is_none() {
                    closest_package = Some(package);
                }
            }
        }

        // No workspace found, but maybe a pyproject.toml was found.
        let package = if let Some(enclosing_package) = closest_package {
            tracing::debug!("Single package workspace at '{}'", enclosing_package.root());

            enclosing_package
        } else {
            tracing::debug!("The ancestor directories contain no `pyproject.toml`. Falling back to a virtual project.");

            // Create a package with a default configuration
            PackageMetadata {
                name: path.file_name().unwrap_or("root").into(),
                root: path.to_path_buf(),
                // TODO create the configuration from the pyproject toml
                configuration: base_configuration.cloned().unwrap_or_default(),
            }
        };

        let root = package.root().to_path_buf();
        let packages = vec![package];
        let settings = packages[0]
            .configuration
            .to_workspace_settings(&root, &packages);

        Ok(Self {
            root,
            packages,
            settings,
        })
    }

    pub fn root(&self) -> &SystemPath {
        &self.root
    }

    pub fn packages(&self) -> &[PackageMetadata] {
        &self.packages
    }

    pub fn settings(&self) -> &WorkspaceSettings {
        &self.settings
    }
}

impl PackageMetadata {
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

        PackageMetadata {
            name,
            root,
            configuration,
        }
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn root(&self) -> &SystemPath {
        &self.root
    }
}

fn collect_packages(
    workspace_package: PackageMetadata,
    workspace_table: &Workspace,
    closest_package: Option<PackageMetadata>,
    base_configuration: Option<&Configuration>,
    system: &dyn System,
) -> Result<CollectedPackagesOrStandalone, WorkspaceDiscoveryError> {
    let workspace_root = workspace_package.root().to_path_buf();
    let mut member_paths = FxHashSet::default();

    for glob in workspace_table.members() {
        let full_glob = workspace_package.root().join(glob);

        let matches = system.glob(full_glob.as_str()).map_err(|error| {
            WorkspaceDiscoveryError::InvalidMembersPattern {
                raw_glob: glob.clone(),
                source: error,
            }
        })?;

        for result in matches {
            let path = result?;
            let normalized = SystemPath::absolute(path, &workspace_root);

            // Skip over non-directory entry. E.g.finder might end up creating a `.DS_STORE` file
            // that ends up matching `/projects/*`.
            if system.is_directory(&normalized) {
                member_paths.insert(normalized);
            } else {
                tracing::debug!("Ignoring non-directory workspace member '{normalized}'");
            }
        }
    }

    // The workspace root is always a member. Don't re-add it
    let mut packages = vec![workspace_package];
    member_paths.remove(&workspace_root);

    // Add the package that is closest to the current working directory except
    // if that package isn't a workspace member, then fallback to creating a single
    // package workspace.
    if let Some(closest_package) = closest_package {
        // the closest `pyproject.toml` isn't a member of this workspace because it is
        // explicitly included or simply not listed.
        // Create a standalone workspace.
        if !member_paths.remove(closest_package.root())
            || workspace_table.is_excluded(closest_package.root(), &workspace_root)?
        {
            tracing::debug!(
                "Ignoring workspace '{workspace_root}' because package '{package}' is not a member",
                package = closest_package.name()
            );
            return Ok(CollectedPackagesOrStandalone::Standalone(closest_package));
        }

        tracing::debug!("adding package '{}'", closest_package.name());
        packages.push(closest_package);
    }

    // Add all remaining member paths
    for member_path in member_paths {
        if workspace_table.is_excluded(&member_path, workspace_root.as_path())? {
            tracing::debug!("Ignoring excluded member '{member_path}'");
            continue;
        }

        let pyproject_path = member_path.join("pyproject.toml");

        let pyproject_str = match system.read_to_string(&pyproject_path) {
            Ok(pyproject_str) => pyproject_str,

            Err(error) => {
                if error.kind() == std::io::ErrorKind::NotFound
                    && member_path
                        .file_name()
                        .is_some_and(|name| name.starts_with('.'))
                {
                    tracing::debug!(
                        "Ignore member '{member_path}' because it has no pyproject.toml and is hidden",
                    );
                    continue;
                }

                return Err(WorkspaceDiscoveryError::MemberFailedToReadPyProject {
                    package_root: member_path,
                    source: error,
                });
            }
        };

        let pyproject = PyProject::from_str(&pyproject_str).map_err(|error| {
            WorkspaceDiscoveryError::InvalidPyProject {
                source: Box::new(error),
                path: pyproject_path,
            }
        })?;

        if pyproject.workspace().is_some() {
            return Err(WorkspaceDiscoveryError::NestedWorkspaces {
                package_root: member_path,
            });
        }

        let package = PackageMetadata::from_pyproject(pyproject, member_path, base_configuration);

        tracing::debug!(
            "Adding package '{}' at '{}'",
            package.name(),
            package.root()
        );

        packages.push(package);
    }

    packages.sort_unstable_by(|a, b| a.root().cmp(b.root()));

    Ok(CollectedPackagesOrStandalone::Packages(packages))
}

enum CollectedPackagesOrStandalone {
    Packages(Vec<PackageMetadata>),
    Standalone(PackageMetadata),
}

#[derive(Debug, Error)]
pub enum WorkspaceDiscoveryError {
    #[error("workspace path '{0}' is not a directory")]
    NotADirectory(SystemPathBuf),

    #[error("nested workspaces aren't supported but the package located at '{package_root}' defines a `knot.workspace` table")]
    NestedWorkspaces { package_root: SystemPathBuf },

    #[error("the workspace contains two packages named '{name}': '{first}' and '{second}'")]
    DuplicatePackageNames {
        name: Name,
        first: SystemPathBuf,
        second: SystemPathBuf,
    },

    #[error("the package '{package_name}' located at '{package_root}' is outside the workspace's root directory '{workspace_root}'")]
    PackageOutsideWorkspace {
        workspace_root: SystemPathBuf,
        package_name: Name,
        package_root: SystemPathBuf,
    },

    #[error(
        "failed to read the `pyproject.toml` for the package located at '{package_root}': {source}"
    )]
    MemberFailedToReadPyProject {
        package_root: SystemPathBuf,
        source: std::io::Error,
    },

    #[error("{path} is not a valid `pyproject.toml`: {source}")]
    InvalidPyProject {
        source: Box<PyProjectError>,
        path: SystemPathBuf,
    },

    #[error("invalid glob '{raw_glob}' in `tool.knot.workspace.members`: {source}")]
    InvalidMembersPattern {
        source: glob::PatternError,
        raw_glob: String,
    },

    #[error("failed to match member glob: {error}")]
    FailedToMatchGlob {
        #[from]
        error: GlobError,
    },
}

#[cfg(test)]
mod tests {
    //! Integration tests for workspace discovery

    use crate::snapshot_workspace;
    use anyhow::Context;
    use insta::assert_ron_snapshot;
    use ruff_db::system::{SystemPathBuf, TestSystem};

    use crate::workspace::{WorkspaceDiscoveryError, WorkspaceMetadata};

    #[test]
    fn package_without_pyproject() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([(root.join("foo.py"), ""), (root.join("bar.py"), "")])
            .context("Failed to write files")?;

        let workspace = WorkspaceMetadata::discover(&root, &system, None)
            .context("Failed to discover workspace")?;

        assert_eq!(workspace.root(), &*root);

        snapshot_workspace!(workspace);

        Ok(())
    }

    #[test]
    fn single_package() -> anyhow::Result<()> {
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

        let workspace = WorkspaceMetadata::discover(&root, &system, None)
            .context("Failed to discover workspace")?;

        assert_eq!(workspace.root(), &*root);
        snapshot_workspace!(workspace);

        // Discovering the same package from a subdirectory should give the same result
        let from_src = WorkspaceMetadata::discover(&root.join("db"), &system, None)
            .context("Failed to discover workspace from src sub-directory")?;

        assert_eq!(from_src, workspace);

        Ok(())
    }

    #[test]
    fn workspace_members() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "workspace-root"

                    [tool.knot.workspace]
                    members = ["packages/*"]
                    exclude = ["packages/excluded"]
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "member-a"
                    "#,
                ),
                (
                    root.join("packages/x/pyproject.toml"),
                    r#"
                    [project]
                    name = "member-x"
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let workspace = WorkspaceMetadata::discover(&root, &system, None)
            .context("Failed to discover workspace")?;

        assert_eq!(workspace.root(), &*root);

        snapshot_workspace!(workspace);

        // Discovering the same package from a member should give the same result
        let from_src = WorkspaceMetadata::discover(&root.join("packages/a"), &system, None)
            .context("Failed to discover workspace from src sub-directory")?;

        assert_eq!(from_src, workspace);

        Ok(())
    }

    #[test]
    fn workspace_excluded() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "workspace-root"

                    [tool.knot.workspace]
                    members = ["packages/*"]
                    exclude = ["packages/excluded"]
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "member-a"
                    "#,
                ),
                (
                    root.join("packages/excluded/pyproject.toml"),
                    r#"
                    [project]
                    name = "member-x"
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let workspace = WorkspaceMetadata::discover(&root, &system, None)
            .context("Failed to discover workspace")?;

        assert_eq!(workspace.root(), &*root);
        snapshot_workspace!(workspace);

        // Discovering the `workspace` for `excluded` should discover a single-package workspace
        let excluded_workspace =
            WorkspaceMetadata::discover(&root.join("packages/excluded"), &system, None)
                .context("Failed to discover workspace from src sub-directory")?;

        assert_ne!(excluded_workspace, workspace);

        Ok(())
    }

    #[test]
    fn workspace_non_unique_member_names() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "workspace-root"

                    [tool.knot.workspace]
                    members = ["packages/*"]
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "a"
                    "#,
                ),
                (
                    root.join("packages/b/pyproject.toml"),
                    r#"
                    [project]
                    name = "a"
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let error = WorkspaceMetadata::discover(&root, &system, None).expect_err(
            "Discovery should error because the workspace contains two packages with the same names.",
        );

        assert_error_eq(&error, "the workspace contains two packages named 'a': '/app/packages/a' and '/app/packages/b'");

        Ok(())
    }

    #[test]
    fn nested_workspaces() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "workspace-root"

                    [tool.knot.workspace]
                    members = ["packages/*"]
                    "#,
                ),
                (
                    root.join("packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "nested-workspace"

                    [tool.knot.workspace]
                    members = ["packages/*"]
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let error = WorkspaceMetadata::discover(&root, &system, None).expect_err(
            "Discovery should error because the workspace has a package that itself is a workspace",
        );

        assert_error_eq(&error, "nested workspaces aren't supported but the package located at '/app/packages/a' defines a `knot.workspace` table");

        Ok(())
    }

    #[test]
    fn member_missing_pyproject_toml() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "workspace-root"

                    [tool.knot.workspace]
                    members = ["packages/*"]
                    "#,
                ),
                (root.join("packages/a/test.py"), ""),
            ])
            .context("Failed to write files")?;

        let error = WorkspaceMetadata::discover(&root, &system, None)
            .expect_err("Discovery should error because member `a` has no `pypyroject.toml`");

        assert_error_eq(&error, "failed to read the `pyproject.toml` for the package located at '/app/packages/a': No such file or directory");

        Ok(())
    }

    /// Folders that match the members pattern but don't have a pyproject.toml
    /// aren't valid members and discovery fails. However, don't fail
    /// if the folder name indicates that it is a hidden folder that might
    /// have been created by another tool
    #[test]
    fn member_pattern_matching_hidden_folder() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "workspace-root"

                    [tool.knot.workspace]
                    members = ["packages/*"]
                    "#,
                ),
                (root.join("packages/.hidden/a.py"), ""),
            ])
            .context("Failed to write files")?;

        let workspace = WorkspaceMetadata::discover(&root, &system, None)?;

        snapshot_workspace!(workspace);

        Ok(())
    }

    #[test]
    fn member_pattern_matching_file() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "workspace-root"

                    [tool.knot.workspace]
                    members = ["packages/*"]
                    "#,
                ),
                (root.join("packages/.DS_STORE"), ""),
            ])
            .context("Failed to write files")?;

        let workspace = WorkspaceMetadata::discover(&root, &system, None)?;

        snapshot_workspace!(&workspace);

        Ok(())
    }

    #[test]
    fn workspace_root_not_an_ancestor_of_member() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/app");

        system
            .memory_file_system()
            .write_files([
                (
                    root.join("pyproject.toml"),
                    r#"
                    [project]
                    name = "workspace-root"

                    [tool.knot.workspace]
                    members = ["../packages/*"]
                    "#,
                ),
                (
                    root.join("../packages/a/pyproject.toml"),
                    r#"
                    [project]
                    name = "a"
                    "#,
                ),
            ])
            .context("Failed to write files")?;

        let error = WorkspaceMetadata::discover(&root, &system, None).expect_err(
            "Discovery should error because member `a` is outside the workspace's directory`",
        );

        assert_error_eq(&error, "the package 'a' located at '/packages/a' is outside the workspace's root directory '/app'");

        Ok(())
    }

    #[track_caller]
    fn assert_error_eq(error: &WorkspaceDiscoveryError, message: &str) {
        assert_eq!(error.to_string().replace('\\', "/"), message);
    }

    /// Snapshots a workspace but with all paths using unix separators.
    #[macro_export]
    macro_rules! snapshot_workspace {
    ($workspace:expr) => {{
        assert_ron_snapshot!($workspace,{
            ".root" => insta::dynamic_redaction(|content, _content_path| {
                content.as_str().unwrap().replace("\\", "/")
            }),
            ".packages[].root" => insta::dynamic_redaction(|content, _content_path| {
                content.as_str().unwrap().replace("\\", "/")
            }),
        });
    }};
}
}
