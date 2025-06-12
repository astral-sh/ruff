use std::sync::Arc;

use ruff_db::{diagnostic::DiagnosticFormat, files::File};
use ty_python_semantic::lint::RuleSelection;

use crate::{Db, combine::Combine, glob::IncludeExcludeFilter, metadata::options::OverrideOptions};

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
    pub(super) files: IncludeExcludeFilter,

    pub(super) options: OverrideOptions,

    /// Pre-resolved rule selection for this override alone.
    /// Used for efficient lookup when only this override matches a file.
    pub(super) settings: Arc<OverrideSettings>,
}

impl Override {
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

/// Resolves the settings for a given file.
#[salsa::tracked(returns(ref))]
pub(crate) fn file_settings(db: &dyn Db, file: File) -> FileSettings {
    let settings = db.project().settings(db);

    let path = match file.path(db) {
        ruff_db::files::FilePath::System(path) => path,
        ruff_db::files::FilePath::SystemVirtual(_) | ruff_db::files::FilePath::Vendored(_) => {
            return FileSettings::Global;
        }
    };

    let mut matching_overrides = settings
        .overrides()
        .iter()
        .filter(|over| over.matches_file(path));

    let Some(first) = matching_overrides.next() else {
        return FileSettings::Global;
    };

    let Some(second) = matching_overrides.next() else {
        return FileSettings::Overriden(Arc::clone(&first.settings));
    };

    // TODO: Should we intern override options to avoid the clone here?
    // Could potential be expensive, but then we only do it once per file.
    let overrides: smallvec::SmallVec<[_; 2]> = [first, second]
        .into_iter()
        .chain(matching_overrides)
        .map(|over| over.options.clone())
        .collect();

    let overrides = ManyOverrides::new(db, &*overrides);
    FileSettings::Overriden(overrides.merge(db))
}

#[salsa::interned]
struct ManyOverrides<'db> {
    overrides: Vec<OverrideOptions>,
}

#[salsa::tracked]
impl<'db> ManyOverrides<'db> {
    #[salsa::tracked]
    fn merge(self, db: &'db dyn Db) -> Arc<OverrideSettings> {
        let mut options = OverrideOptions {
            rules: db.project().metadata(db).options().rules.clone(),
        };

        for option in self.overrides(db) {
            options.combine_with(option);
        }

        let rules = if let Some(rules) = options.rules {
            rules.to_rule_selection(db, &mut Vec::new())
        } else {
            RuleSelection::from_registry(db.lint_registry())
        };

        Arc::new(OverrideSettings { rules })
    }
}

/// The resolved settings for a file.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum FileSettings {
    Global,
    Overriden(Arc<OverrideSettings>),
}

impl FileSettings {
    pub fn rules<'a>(&'a self, db: &'a dyn Db) -> &'a RuleSelection {
        match self {
            FileSettings::Global => db.project().settings(db).rules(),
            FileSettings::Overriden(override_settings) => &override_settings.rules,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct OverrideSettings {
    pub(super) rules: RuleSelection,
}
