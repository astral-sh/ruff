use std::sync::Arc;

use ruff_db::files::File;
use ty_combine::Combine;
use ty_python_semantic::AnalysisSettings;
use ty_python_semantic::lint::RuleSelection;

use crate::metadata::options::{InnerOverrideOptions, OutputFormat};
use crate::{Db, glob::IncludeExcludeFilter};

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
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct Settings {
    pub(super) rules: Arc<RuleSelection>,
    pub(super) terminal: TerminalSettings,
    pub(super) src: SrcSettings,
    pub(super) analysis: AnalysisSettings,

    /// Settings for configuration overrides that apply to specific file patterns.
    ///
    /// Each override can specify include/exclude patterns and rule configurations
    /// that apply to matching files. Multiple overrides can match the same file,
    /// with later overrides taking precedence.
    pub(super) overrides: Vec<Override>,
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

    pub fn overrides(&self) -> &[Override] {
        &self.overrides
    }

    pub fn analysis(&self) -> &AnalysisSettings {
        &self.analysis
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, get_size2::GetSize)]
pub struct TerminalSettings {
    pub output_format: OutputFormat,
    pub error_on_warning: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct SrcSettings {
    pub respect_ignore_files: bool,
    pub files: IncludeExcludeFilter,
}

/// A single configuration override that applies to files matching specific patterns.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct Override {
    /// File pattern filter to determine which files this override applies to.
    pub(super) files: IncludeExcludeFilter,

    /// The raw options as specified in the configuration (minus `include` and `exclude`.
    /// Necessary to merge multiple overrides if necessary.
    pub(super) options: Arc<InnerOverrideOptions>,

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
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
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
        // If the file matches no override, it uses the global settings.
        return FileSettings::Global;
    };

    let Some(second) = matching_overrides.next() else {
        tracing::debug!("Applying override for file `{path}`: {}", first.files);
        // If the file matches only one override, return that override's settings.
        return FileSettings::File(Arc::clone(&first.settings));
    };

    let mut filters = tracing::enabled!(tracing::Level::DEBUG)
        .then(|| format!("({}), ({})", first.files, second.files));

    let mut overrides = vec![Arc::clone(&first.options), Arc::clone(&second.options)];

    for over in matching_overrides {
        use std::fmt::Write;

        if let Some(filters) = &mut filters {
            let _ = write!(filters, ", ({})", over.files);
        }

        overrides.push(Arc::clone(&over.options));
    }

    if let Some(filters) = &filters {
        tracing::debug!("Applying multiple overrides for file `{path}`: {filters}");
    }

    merge_overrides(db, overrides, ())
}

/// Merges multiple override options, caching the result.
///
/// Overrides often apply to multiple files. This query ensures that we avoid
/// resolving the same override combinations multiple times.
///
/// ## What's up with the `()` argument?
///
/// This is to make Salsa happy because it requires that queries with only a single argument
/// take a salsa-struct as argument, which isn't the case here. The `()` enables salsa's
/// automatic interning for the arguments.
#[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
fn merge_overrides(db: &dyn Db, overrides: Vec<Arc<InnerOverrideOptions>>, _: ()) -> FileSettings {
    let mut overrides = overrides.into_iter().rev();
    let mut merged = (*overrides.next().unwrap()).clone();

    for option in overrides {
        merged.combine_with((*option).clone());
    }

    merged
        .rules
        .combine_with(db.project().metadata(db).options().rules.clone());

    let Some(rules) = merged.rules else {
        return FileSettings::Global;
    };

    // It's okay to ignore the errors here because the rules are eagerly validated
    // during `overrides.to_settings()`.
    let rules = rules.to_rule_selection(db, &mut Vec::new());
    FileSettings::File(Arc::new(OverrideSettings { rules }))
}

/// The resolved settings for a file.
#[derive(Debug, Eq, PartialEq, Clone, get_size2::GetSize)]
pub enum FileSettings {
    /// The file uses the global settings.
    Global,

    /// The file has specific override settings.
    File(Arc<OverrideSettings>),
}

impl FileSettings {
    pub fn rules<'a>(&'a self, db: &'a dyn Db) -> &'a RuleSelection {
        match self {
            FileSettings::Global => db.project().settings(db).rules(),
            FileSettings::File(override_settings) => &override_settings.rules,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, get_size2::GetSize)]
pub struct OverrideSettings {
    pub(super) rules: RuleSelection,
}
