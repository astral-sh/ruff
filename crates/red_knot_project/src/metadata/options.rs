use crate::metadata::value::{RelativePathBuf, ValueSource, ValueSourceGuard};
use red_knot_python_semantic::{
    ProgramSettings, PythonPlatform, PythonVersion, SearchPathSettings, SitePackages,
};
use ruff_db::system::{System, SystemPath};
use ruff_macros::Combine;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use thiserror::Error;

/// The options for the project.
#[derive(Debug, Default, Clone, PartialEq, Eq, Combine, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Options {
    pub environment: Option<EnvironmentOptions>,

    pub src: Option<SrcOptions>,
}

impl Options {
    pub(crate) fn from_toml_str(content: &str, source: ValueSource) -> Result<Self, KnotTomlError> {
        let _guard = ValueSourceGuard::new(source);
        let options = toml::from_str(content)?;
        Ok(options)
    }

    pub(crate) fn to_program_settings(
        &self,
        project_root: &SystemPath,
        system: &dyn System,
    ) -> ProgramSettings {
        let (python_version, python_platform) = self
            .environment
            .as_ref()
            .map(|env| (env.python_version, env.python_platform.as_ref()))
            .unwrap_or_default();

        ProgramSettings {
            python_version: python_version.unwrap_or_default(),
            python_platform: python_platform.cloned().unwrap_or_default(),
            search_paths: self.to_search_path_settings(project_root, system),
        }
    }

    fn to_search_path_settings(
        &self,
        project_root: &SystemPath,
        system: &dyn System,
    ) -> SearchPathSettings {
        let src_roots = if let Some(src_root) = self.src.as_ref().and_then(|src| src.root.as_ref())
        {
            vec![src_root.absolute(project_root, system)]
        } else {
            let src = project_root.join("src");

            // Default to `src` and the project root if `src` exists and the root hasn't been specified.
            if system.is_directory(&src) {
                vec![project_root.to_path_buf(), src]
            } else {
                vec![project_root.to_path_buf()]
            }
        };

        let (extra_paths, python, typeshed) = self
            .environment
            .as_ref()
            .map(|env| {
                (
                    env.extra_paths.clone(),
                    env.venv_path.clone(),
                    env.typeshed.clone(),
                )
            })
            .unwrap_or_default();

        SearchPathSettings {
            extra_paths: extra_paths
                .unwrap_or_default()
                .into_iter()
                .map(|path| path.absolute(project_root, system))
                .collect(),
            src_roots,
            typeshed: typeshed.map(|path| path.absolute(project_root, system)),
            site_packages: python
                .map(|venv_path| SitePackages::Derived {
                    venv_path: venv_path.absolute(project_root, system),
                })
                .unwrap_or(SitePackages::Known(vec![])),
        }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EnvironmentOptions {
    pub python_version: Option<PythonVersion>,

    pub python_platform: Option<PythonPlatform>,

    /// List of user-provided paths that should take first priority in the module resolution.
    /// Examples in other type checkers are mypy's MYPYPATH environment variable,
    /// or pyright's stubPath configuration setting.
    pub extra_paths: Option<Vec<RelativePathBuf>>,

    /// Optional path to a "typeshed" directory on disk for us to use for standard-library types.
    /// If this is not provided, we will fallback to our vendored typeshed stubs for the stdlib,
    /// bundled as a zip file in the binary
    pub typeshed: Option<RelativePathBuf>,

    // TODO: Rename to python, see https://github.com/astral-sh/ruff/issues/15530
    /// The path to the user's `site-packages` directory, where third-party packages from ``PyPI`` are installed.
    pub venv_path: Option<RelativePathBuf>,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Combine, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SrcOptions {
    /// The root of the project, used for finding first-party modules.
    pub root: Option<RelativePathBuf>,
}

#[derive(Error, Debug)]
pub enum KnotTomlError {
    #[error(transparent)]
    TomlSyntax(#[from] toml::de::Error),
}
