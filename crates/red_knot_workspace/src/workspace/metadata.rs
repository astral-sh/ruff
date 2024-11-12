use crate::workspace::pyproject::{PyProject, Workspace};
use crate::workspace::settings::{Configuration, WorkspaceSettings};
use anyhow::{anyhow, Context};

use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_python_ast::name::Name;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

#[derive(Debug)]
pub struct WorkspaceMetadata {
    pub(super) root: SystemPathBuf,

    /// The (first-party) packages in this workspace.
    pub(super) packages: Vec<PackageMetadata>,

    /// The resolved settings for this workspace.
    pub(super) settings: WorkspaceSettings,
}

/// A first-party package in a workspace.
#[derive(Debug, Clone)]
pub struct PackageMetadata {
    pub(super) name: Name,

    /// The path to the root directory of the package.
    pub(super) root: SystemPathBuf,

    pub(super) configuration: Configuration,
}

impl WorkspaceMetadata {
    /// Discovers the closest workspace at `path` and returns its metadata.
    pub fn discover(
        path: &SystemPath,
        system: &dyn System,
        base_configuration: Option<&Configuration>,
    ) -> anyhow::Result<WorkspaceMetadata> {
        assert!(
            system.is_directory(path),
            "Workspace root path must be a directory"
        );
        tracing::debug!("Searching for a workspace in '{path}'");

        // TODO: Issue with this implementation: We have to skip the member if it is excluded.
        let mut closest_package: Option<PackageMetadata> = None;

        for ancestor in path.ancestors() {
            let pyproject_path = ancestor.join("pyproject.toml");
            if let Ok(pyproject_str) = system.read_to_string(&pyproject_path) {
                let pyproject = PyProject::from_str(&pyproject_str)
                    .with_context(|| format!("Failed to parse '{pyproject_path}'"))?;

                // TODO: Extract this into a workspace configuration?
                let workspace_table = pyproject.workspace().cloned();
                let package = PackageMetadata::from_pyproject(
                    pyproject,
                    ancestor.to_path_buf(),
                    base_configuration,
                )?;

                if let Some(workspace_table) = workspace_table {
                    tracing::debug!("Found workspace at '{}'", ancestor);

                    match collect_packages(
                        package,
                        &workspace_table,
                        closest_package,
                        base_configuration,
                        system,
                    )? {
                        CollectedPackagesOrStandalone::Packages(packages) => {
                            let mut by_name =
                                FxHashMap::with_capacity_and_hasher(packages.len(), FxBuildHasher);

                            let mut workspace_package = None;

                            for package in &packages {
                                if let Some(conflicting) = by_name.insert(package.name(), package) {
                                    return Err(anyhow!(
                                        "The workspace contains two packages named '{name}': '{first}' and '{second}'",
                                        name=package.name(),
                                        first=conflicting.root(),
                                        second=package.root()
                                    ));
                                }

                                if package.root() == ancestor {
                                    workspace_package = Some(package);
                                }
                            }

                            let workspace_package = workspace_package
                                .expect("workspace package to be part of the workspace's packages");

                            let settings = workspace_package
                                .configuration
                                .to_workspace_settings(ancestor, &packages);

                            return Ok(Self {
                                root: ancestor.to_path_buf(),
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
                name: path.file_name().unwrap_or("<virtual>").into(),
                root: path.to_path_buf(),
                // TODO create the configuration from the pyproject toml
                configuration: base_configuration.cloned().unwrap_or_default(),
            }
        };

        let root = package.root().to_path_buf();
        let packages = vec![package];
        let settings = packages[0]
            .configuration
            .to_workspace_settings(path, &packages);

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
    ) -> anyhow::Result<Self> {
        let Some(project) = pyproject.project else {
            return Err(anyhow!("`[project]` table is missing"));
        };

        // TODO: Should we allow pyproject.toml without a name and default to the directory
        // name instead?

        let Some(name) = project.name.as_ref() else {
            return Err(anyhow!("`project.name` is missing"));
        };

        // TODO: load configuration from pyrpoject.toml
        let mut configuration = Configuration::default();

        if let Some(base_configuration) = base_configuration {
            configuration.extend(base_configuration.clone());
        }

        Ok(PackageMetadata {
            name: Name::from(name.as_str()),
            root,
            configuration,
        })
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn root(&self) -> &SystemPath {
        &self.root
    }
}

// TODO: paths don't get normalized
//   Adding package 'bar' at '/Users/micha/astral/test/./symlink/bar'
fn collect_packages(
    workspace_package: PackageMetadata,
    workspace_table: &Workspace,
    closest_package: Option<PackageMetadata>,
    base_configuration: Option<&Configuration>,
    system: &dyn System,
) -> anyhow::Result<CollectedPackagesOrStandalone> {
    let workspace_root = workspace_package.root().to_path_buf();
    let mut member_paths = FxHashSet::default();

    for glob in workspace_table.members() {
        let full_glob = workspace_package.root().join(glob);

        for result in system
            .glob(full_glob.as_str())
            .with_context(|| format!("failed to parse members glob '{glob}'"))?
        {
            let path = result.context("failed to match glob")?;

            // Skip over non-directory entry. E.g.finder might end up creating a `.DS_STORE` file
            // that ends up matching `/projects/*`.
            if system.is_directory(&path) {
                member_paths.insert(path);
            } else {
                tracing::debug!("Ignoring non-directory workspace member '{path}'");
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

                return Err(error).with_context(|| {
                    format!("pyproject.toml for member '{member_path}' is missing")
                })?;
            }
        };

        let pyproject = PyProject::from_str(&pyproject_str)
            .with_context(|| format!("failed to parse '{pyproject_path}'"))?;

        if pyproject.workspace().is_some() {
            anyhow::bail!(
                "Workspace member '{}' is a workspace itself. Nested workspaces aren't supported",
                member_path
            );
        }

        let package = PackageMetadata::from_pyproject(pyproject, member_path, base_configuration)
            .with_context(|| format!("failed to load '{pyproject_path}'"))?;

        tracing::debug!(
            "Adding package '{}' at '{}'",
            package.name(),
            package.root()
        );

        packages.push(package);
    }

    Ok(CollectedPackagesOrStandalone::Packages(packages))
}

enum CollectedPackagesOrStandalone {
    Packages(Vec<PackageMetadata>),
    Standalone(PackageMetadata),
}
