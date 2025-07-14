use super::options::DiagnosticMode;

use ty_project::metadata::options::ProjectOptionsOverrides;

/// Resolved client settings for a specific document. These settings are meant to be
/// used directly by the server, and are *not* a 1:1 representation with how the client
/// sends them.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct ClientSettings {
    pub(super) disable_language_services: bool,
    pub(super) diagnostic_mode: DiagnosticMode,
    pub(super) overrides: Option<ProjectOptionsOverrides>,
}

impl ClientSettings {
    pub(crate) fn is_language_services_disabled(&self) -> bool {
        self.disable_language_services
    }

    pub(crate) fn diagnostic_mode(&self) -> DiagnosticMode {
        self.diagnostic_mode
    }

    pub(crate) fn project_options_overrides(&self) -> Option<&ProjectOptionsOverrides> {
        self.overrides.as_ref()
    }
}
