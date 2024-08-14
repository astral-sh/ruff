use red_knot_python_semantic::{ProgramSettings, PythonVersion, SearchPathSettings};
use ruff_db::system::{System, SystemPath, SystemPathBuf};

use crate::site_packages::VirtualEnvironment;

#[derive(Debug, Default, Clone)]
pub struct WorkspaceConfiguration {
    pub target_version: Option<PythonVersion>,
    pub search_paths: SearchPathConfiguration,
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct SearchPathConfiguration {
    /// List of user-provided paths that should take first priority in the module resolution.
    /// Examples in other type checkers are mypy's MYPYPATH environment variable,
    /// or pyright's stubPath configuration setting.
    pub extra_paths: Option<Vec<SystemPathBuf>>,

    /// The root of the workspace, used for finding first-party modules.
    pub src_root: Option<SystemPathBuf>,

    /// Optional path to a "custom typeshed" directory on disk for us to use for standard-library types.
    /// If this is not provided, we will fallback to our vendored typeshed stubs for the stdlib,
    /// bundled as a zip file in the binary
    pub custom_typeshed: Option<SystemPathBuf>,

    /// The path to the user's `site-packages` directory, where third-party packages from ``PyPI`` are installed.
    pub site_packages: Option<SitePackages>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SitePackages {
    Derived {
        venv_path: SystemPathBuf,
    },
    /// Resolved site packages path for testing only
    Known(SystemPathBuf),
}

impl WorkspaceConfiguration {
    pub fn to_program_settings(
        &self,
        workspace_root: &SystemPath,
        system: &dyn System,
    ) -> anyhow::Result<ProgramSettings> {
        let search_path_settings = self.search_paths.to_settings(workspace_root, system)?;

        Ok(ProgramSettings {
            target_version: self.target_version.unwrap_or_default(),
            search_paths: search_path_settings,
        })
    }
}

impl SearchPathConfiguration {
    pub fn to_settings(
        &self,
        workspace_root: &SystemPath,
        system: &dyn System,
    ) -> anyhow::Result<SearchPathSettings> {
        let site_packages = self
            .site_packages
            .as_ref()
            .map(|site_packages| match site_packages {
                SitePackages::Derived { venv_path } => VirtualEnvironment::new(venv_path, system)
                    .and_then(|venv| venv.site_packages_directories(system)),
                SitePackages::Known(path) => Ok(vec![path.clone()]),
            })
            .transpose()?
            .unwrap_or_default();

        Ok(SearchPathSettings {
            extra_paths: self.extra_paths.clone().unwrap_or_default(),
            src_root: self
                .src_root
                .clone()
                .unwrap_or_else(|| workspace_root.to_path_buf()),
            custom_typeshed: self.custom_typeshed.clone(),
            site_packages,
        })
    }
}

pub trait WorkspaceConfigurationTransformer {
    fn transform(&self, workspace_configuration: WorkspaceConfiguration) -> WorkspaceConfiguration;
}

impl WorkspaceConfigurationTransformer for () {
    fn transform(&self, workspace_configuration: WorkspaceConfiguration) -> WorkspaceConfiguration {
        workspace_configuration
    }
}

impl<T> WorkspaceConfigurationTransformer for T
where
    T: Fn(WorkspaceConfiguration) -> WorkspaceConfiguration,
{
    fn transform(&self, workspace_configuration: WorkspaceConfiguration) -> WorkspaceConfiguration {
        self(workspace_configuration)
    }
}
