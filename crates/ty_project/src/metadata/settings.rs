use std::sync::Arc;

use ruff_db::diagnostic::DiagnosticFormat;
use ty_python_semantic::lint::RuleSelection;

use crate::glob::IncludeExcludeFilter;

/// The resolved [`super::Options`] for the project.
///
/// Unlike [`super::Options`], the struct has default values filled in and
/// uses representations that are optimized for reads (instead of preserving the source representation).
/// It's also not required that this structure precisely resembles the TOML schema, although
/// it's encouraged to use a similar structure.
///
/// It's worth considering to adding a salsa query for specific settings to
/// limit the blast radius when only some settings change. For example,
/// changing the terminal settings shouldn't invalidate any core type-checking queries.
/// This can be achieved by adding a salsa query for the type checking specific settings.
///
/// Settings that are part of [`ty_python_semantic::ProgramSettings`] are not included here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settings {
    pub(super) rules: Arc<RuleSelection>,
    pub(super) terminal: TerminalSettings,
    pub(super) src: SrcSettings,
}

impl Settings {
    pub fn rules(&self) -> &RuleSelection {
        &self.rules
    }

    pub fn src(&self) -> &SrcSettings {
        &self.src
    }

    pub fn to_rules(&self) -> Arc<RuleSelection> {
        self.rules.clone()
    }

    pub fn terminal(&self) -> &TerminalSettings {
        &self.terminal
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TerminalSettings {
    pub output_format: DiagnosticFormat,
    pub error_on_warning: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrcSettings {
    pub respect_ignore_files: bool,
    pub files: IncludeExcludeFilter,
}
