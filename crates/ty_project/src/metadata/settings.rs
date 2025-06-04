use ruff_db::diagnostic::DiagnosticFormat;
use std::sync::Arc;
use ty_python_semantic::lint::RuleSelection;

use crate::walk::FilePatterns;

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
    rules: Arc<RuleSelection>,

    terminal: TerminalSettings,

    src: SrcSettings,
}

impl Settings {
    pub fn new(rules: RuleSelection) -> Self {
        Self {
            rules: Arc::new(rules),
            terminal: TerminalSettings::default(),
            src: SrcSettings::default(),
        }
    }

    pub fn rules(&self) -> &RuleSelection {
        &self.rules
    }

    pub fn src(&self) -> &SrcSettings {
        &self.src
    }

    pub fn set_src(&mut self, src: SrcSettings) {
        self.src = src;
    }

    pub fn to_rules(&self) -> Arc<RuleSelection> {
        self.rules.clone()
    }

    pub fn terminal(&self) -> &TerminalSettings {
        &self.terminal
    }

    pub fn set_terminal(&mut self, terminal: TerminalSettings) {
        self.terminal = terminal;
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

    pub files: FilePatterns,
}

impl Default for SrcSettings {
    fn default() -> Self {
        Self {
            respect_ignore_files: true,
            // TODO: This should include all files by default
            files: FilePatterns::empty(),
        }
    }
}
