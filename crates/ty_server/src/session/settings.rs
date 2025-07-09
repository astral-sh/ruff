use super::options::DiagnosticMode;

use crate::session::options::ActiveEnvironment;
use ruff_db::system::SystemPathBuf;
use ruff_python_ast::PythonVersion;
use ty_project::metadata::Options;
use ty_project::metadata::options::ProjectOptionsOverrides;
use ty_project::metadata::value::{RangedValue, RelativePathBuf};

/// Resolved client settings for a specific document. These settings are meant to be
/// used directly by the server, and are *not* a 1:1 representation with how the client
/// sends them.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct ClientSettings {
    pub(super) disable_language_services: bool,
    pub(super) diagnostic_mode: DiagnosticMode,
    pub(super) active_python_environment: Option<ActiveEnvironment>,
}

impl ClientSettings {
    pub(crate) fn is_language_services_disabled(&self) -> bool {
        self.disable_language_services
    }

    pub(crate) fn diagnostic_mode(&self) -> DiagnosticMode {
        self.diagnostic_mode
    }

    pub(crate) fn to_project_overrides(&self) -> Option<ProjectOptionsOverrides> {
        let Some(active_environment) = &self.active_python_environment else {
            return None;
        };

        let mut overrides = ProjectOptionsOverrides::new(None, Options::default());

        overrides.fallback_python = if let Some(environment) = &active_environment.environment {
            environment.folder_uri.to_file_path().ok().and_then(|path| {
                Some(RelativePathBuf::python_extension(
                    SystemPathBuf::from_path_buf(path).ok()?,
                ))
            })
        } else {
            Some(RelativePathBuf::python_extension(
                active_environment.executable.sys_prefix.clone(),
            ))
        };

        overrides.fallback_python_version =
            active_environment.version.as_ref().and_then(|version| {
                Some(RangedValue::python_extension(PythonVersion::from((
                    u8::try_from(version.major).ok()?,
                    u8::try_from(version.minor).ok()?,
                ))))
            });

        Some(overrides)
    }
}
