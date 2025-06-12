use std::sync::Arc;

use ruff_db::diagnostic::DiagnosticFormat;
use ty_python_semantic::lint::RuleSelection;

use super::options::Rules;
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
    pub(super) overrides: OverridesSettings,
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

    pub fn overrides(&self) -> &OverridesSettings {
        &self.overrides
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

/// Settings for configuration overrides that apply to specific file patterns.
///
/// Each override can specify include/exclude patterns and rule configurations
/// that apply to matching files. Multiple overrides can match the same file,
/// with later overrides taking precedence.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OverridesSettings {
    pub overrides: Vec<Override>,
}

impl OverridesSettings {
    pub fn new(overrides: Vec<Override>) -> Self {
        Self { overrides }
    }

    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Override> {
        self.overrides.iter()
    }
}

/// A single configuration override that applies to files matching specific patterns.
///
/// Each override contains:
/// - File patterns (include/exclude) to determine which files it applies to
/// - Raw rule options as specified in configuration
/// - Pre-resolved rule selection for efficient lookup when only this override matches
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    /// File pattern filter to determine which files this override applies to.
    pub files: IncludeExcludeFilter,

    /// Raw rule options as specified in the configuration.
    /// Used when multiple overrides match a file and need to be merged.
    pub rules: Option<Rules>,

    /// Pre-resolved rule selection for this override alone.
    /// Used for efficient lookup when only this override matches a file.
    pub rule_selection: Arc<RuleSelection>,
}

impl Override {
    pub fn new(
        files: IncludeExcludeFilter,
        rules: Option<Rules>,
        rule_selection: Arc<RuleSelection>,
    ) -> Self {
        Self {
            files,
            rules,
            rule_selection,
        }
    }

    /// Returns whether this override applies to the given file path.
    pub fn matches_file(&self, path: &ruff_db::system::SystemPath) -> bool {
        use crate::glob::{GlobFilterCheckMode, IncludeResult};

        matches!(
            self.files
                .is_file_included(path, GlobFilterCheckMode::Adhoc),
            IncludeResult::Included
        )
    }
}
