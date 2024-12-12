use crate::workspace::PackageMetadata;
use red_knot_python_semantic::{ProgramSettings, PythonVersion, SearchPathSettings, SitePackages};
use ruff_db::system::{SystemPath, SystemPathBuf};

/// The resolved configurations.
///
/// The main difference to [`Configuration`] is that default values are filled in.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct WorkspaceSettings {
    pub(super) program: ProgramSettings,
}

impl WorkspaceSettings {
    pub fn program(&self) -> &ProgramSettings {
        &self.program
    }
}

/// The configuration for the workspace or a package.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct Configuration {
    pub python_version: Option<PythonVersion>,
    pub search_paths: SearchPathConfiguration,
}

impl Configuration {
    /// Extends this configuration by using the values from `with` for all values that are absent in `self`.
    pub fn extend(&mut self, with: Configuration) {
        self.python_version = self.python_version.or(with.python_version);
        self.search_paths.extend(with.search_paths);
    }

    pub fn to_workspace_settings(
        &self,
        workspace_root: &SystemPath,
        _packages: &[PackageMetadata],
    ) -> WorkspaceSettings {
        WorkspaceSettings {
            program: ProgramSettings {
                python_version: self.python_version.unwrap_or_default(),
                search_paths: self.search_paths.to_settings(workspace_root),
            },
        }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(serde::Serialize))]
pub struct SearchPathConfiguration {
    /// List of user-provided paths that should take first priority in the module resolution.
    /// Examples in other type checkers are mypy's MYPYPATH environment variable,
    /// or pyright's stubPath configuration setting.
    pub extra_paths: Option<Vec<SystemPathBuf>>,

    /// The root of the workspace, used for finding first-party modules.
    pub src_root: Option<SystemPathBuf>,

    /// Optional path to a "typeshed" directory on disk for us to use for standard-library types.
    /// If this is not provided, we will fallback to our vendored typeshed stubs for the stdlib,
    /// bundled as a zip file in the binary
    pub typeshed: Option<SystemPathBuf>,

    /// The path to the user's `site-packages` directory, where third-party packages from ``PyPI`` are installed.
    pub site_packages: Option<SitePackages>,
}

impl SearchPathConfiguration {
    pub fn to_settings(&self, workspace_root: &SystemPath) -> SearchPathSettings {
        let site_packages = self
            .site_packages
            .clone()
            .unwrap_or(SitePackages::Known(vec![]));

        SearchPathSettings {
            extra_paths: self.extra_paths.clone().unwrap_or_default(),
            src_root: self
                .clone()
                .src_root
                .unwrap_or_else(|| workspace_root.to_path_buf()),
            typeshed: self.typeshed.clone(),
            site_packages,
        }
    }

    pub fn extend(&mut self, with: SearchPathConfiguration) {
        if let Some(extra_paths) = with.extra_paths {
            self.extra_paths.get_or_insert(extra_paths);
        }
        if let Some(src_root) = with.src_root {
            self.src_root.get_or_insert(src_root);
        }
        if let Some(typeshed) = with.typeshed {
            self.typeshed.get_or_insert(typeshed);
        }
        if let Some(site_packages) = with.site_packages {
            self.site_packages.get_or_insert(site_packages);
        }
    }
}
