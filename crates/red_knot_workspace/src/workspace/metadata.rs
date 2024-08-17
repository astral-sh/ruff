use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_python_ast::name::Name;

#[derive(Debug)]
pub struct WorkspaceMetadata {
    pub(super) root: SystemPathBuf,

    /// The (first-party) packages in this workspace.
    pub(super) packages: Vec<PackageMetadata>,
}

/// A first-party package in a workspace.
#[derive(Debug)]
pub struct PackageMetadata {
    pub(super) name: Name,

    /// The path to the root directory of the package.
    pub(super) root: SystemPathBuf,
    // TODO: Add the loaded package configuration (not the nested ruff settings)
}

impl WorkspaceMetadata {
    /// Discovers the closest workspace at `path` and returns its metadata.
    pub fn from_path(path: &SystemPath, system: &dyn System) -> anyhow::Result<WorkspaceMetadata> {
        assert!(
            system.is_directory(path),
            "Workspace root path must be a directory"
        );
        tracing::debug!("Searching for workspace in '{path}'");

        let root = path.to_path_buf();

        // TODO: Discover package name from `pyproject.toml`.
        let package_name: Name = path.file_name().unwrap_or("<root>").into();

        let package = PackageMetadata {
            name: package_name,
            root: root.clone(),
        };

        let workspace = WorkspaceMetadata {
            root,
            packages: vec![package],
        };

        Ok(workspace)
    }

    pub fn root(&self) -> &SystemPath {
        &self.root
    }

    pub fn packages(&self) -> &[PackageMetadata] {
        &self.packages
    }
}

impl PackageMetadata {
    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn root(&self) -> &SystemPath {
        &self.root
    }
}
