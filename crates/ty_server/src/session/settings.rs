use super::options::DiagnosticMode;

use ty_ide::InlayHintSettings;
use ty_project::metadata::options::ProjectOptionsOverrides;

/// Resolved client settings that are shared across all workspaces.
#[derive(Clone, Default, Debug, PartialEq)]
pub(crate) struct GlobalSettings {
    pub(super) diagnostic_mode: DiagnosticMode,
    pub(super) experimental: ExperimentalSettings,
}

impl GlobalSettings {
    pub(crate) fn is_rename_enabled(&self) -> bool {
        self.experimental.rename
    }

    pub(crate) fn is_auto_import_enabled(&self) -> bool {
        self.experimental.auto_import
    }
}

impl GlobalSettings {
    pub(crate) fn diagnostic_mode(&self) -> DiagnosticMode {
        self.diagnostic_mode
    }
}

#[derive(Clone, Default, Debug, PartialEq)]
pub(crate) struct ExperimentalSettings {
    pub(super) rename: bool,
    pub(super) auto_import: bool,
}

/// Resolved client settings for a specific workspace.
///
/// These settings are meant to be used directly by the server, and are *not* a 1:1 representation
/// with how the client sends them.
#[derive(Clone, Default, Debug)]
pub(crate) struct WorkspaceSettings {
    pub(super) disable_language_services: bool,
    pub(super) inlay_hints: InlayHintSettings,
    pub(super) overrides: Option<ProjectOptionsOverrides>,
}

impl WorkspaceSettings {
    pub(crate) fn is_language_services_disabled(&self) -> bool {
        self.disable_language_services
    }

    pub(crate) fn project_options_overrides(&self) -> Option<&ProjectOptionsOverrides> {
        self.overrides.as_ref()
    }

    pub(crate) fn inlay_hints(&self) -> &InlayHintSettings {
        &self.inlay_hints
    }
}
