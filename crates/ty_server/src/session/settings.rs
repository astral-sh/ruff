use super::options::DiagnosticMode;

use ty_project::metadata::options::ProjectOptionsOverrides;

/// Resolved client settings that are shared across all workspaces.
#[derive(Clone, Default, Debug, PartialEq)]
pub(crate) struct GlobalSettings {
    pub(super) diagnostic_mode: DiagnosticMode,
}

impl GlobalSettings {
    pub(crate) fn diagnostic_mode(&self) -> DiagnosticMode {
        self.diagnostic_mode
    }
}

/// Resolved client settings for a specific workspace.
///
/// These settings are meant to be used directly by the server, and are *not* a 1:1 representation
/// with how the client sends them.
#[derive(Clone, Default, Debug)]
pub(crate) struct WorkspaceSettings {
    pub(super) disable_language_services: bool,
    pub(super) overrides: Option<ProjectOptionsOverrides>,
}

impl WorkspaceSettings {
    pub(crate) fn is_language_services_disabled(&self) -> bool {
        self.disable_language_services
    }

    pub(crate) fn project_options_overrides(&self) -> Option<&ProjectOptionsOverrides> {
        self.overrides.as_ref()
    }
}
