use core::fmt;
use itertools::Itertools;
use ruff_db::diagnostic::{DiagnosticId, LintName, Severity};
use rustc_hash::FxHashMap;
use std::fmt::Formatter;
use std::hash::Hasher;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct LintMetadata {
    /// The unique identifier for the lint.
    pub name: LintName,

    /// A one-sentence summary of what the lint catches.
    pub summary: &'static str,

    /// An in depth explanation of the lint in markdown. Covers what the lint does, why it's bad and possible fixes.
    ///
    /// The documentation may require post-processing to be rendered correctly. For example, lines
    /// might have leading or trailing whitespace that should be removed.
    pub raw_documentation: &'static str,

    /// The default level of the lint if the user doesn't specify one.
    pub default_level: Level,

    pub status: LintStatus,

    /// The source file in which the lint is declared.
    pub file: &'static str,

    /// The 1-based line number in the source `file` where the lint is declared.
    pub line: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Level {
    /// # Ignore
    ///
    /// The lint is disabled and should not run.
    Ignore,

    /// # Warn
    ///
    /// The lint is enabled and diagnostic should have a warning severity.
    Warn,

    /// # Error
    ///
    /// The lint is enabled and diagnostics have an error severity.
    Error,
}

impl Level {
    pub const fn is_error(self) -> bool {
        matches!(self, Level::Error)
    }

    pub const fn is_warn(self) -> bool {
        matches!(self, Level::Warn)
    }

    pub const fn is_ignore(self) -> bool {
        matches!(self, Level::Ignore)
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Level::Ignore => f.write_str("ignore"),
            Level::Warn => f.write_str("warn"),
            Level::Error => f.write_str("error"),
        }
    }
}

impl TryFrom<Level> for Severity {
    type Error = ();

    fn try_from(level: Level) -> Result<Self, ()> {
        match level {
            Level::Ignore => Err(()),
            Level::Warn => Ok(Severity::Warning),
            Level::Error => Ok(Severity::Error),
        }
    }
}

impl LintMetadata {
    pub fn name(&self) -> LintName {
        self.name
    }

    pub fn summary(&self) -> &str {
        self.summary
    }

    /// Returns the documentation line by line with one leading space and all trailing whitespace removed.
    pub fn documentation_lines(&self) -> impl Iterator<Item = &str> {
        self.raw_documentation.lines().map(|line| {
            line.strip_prefix(char::is_whitespace)
                .unwrap_or(line)
                .trim_end()
        })
    }

    /// Returns the documentation as a single string.
    pub fn documentation(&self) -> String {
        self.documentation_lines().join("\n")
    }

    pub fn default_level(&self) -> Level {
        self.default_level
    }

    pub fn status(&self) -> &LintStatus {
        &self.status
    }

    pub fn file(&self) -> &str {
        self.file
    }

    pub fn line(&self) -> u32 {
        self.line
    }
}

#[doc(hidden)]
pub const fn lint_metadata_defaults() -> LintMetadata {
    LintMetadata {
        name: LintName::of(""),
        summary: "",
        raw_documentation: "",
        default_level: Level::Error,
        status: LintStatus::preview("0.0.0"),
        file: "",
        line: 1,
    }
}

#[derive(Copy, Clone, Debug)]
pub enum LintStatus {
    /// The lint has been added to the linter, but is not yet stable.
    Preview {
        /// The version in which the lint was added.
        since: &'static str,
    },

    /// The lint is stable.
    Stable {
        /// The version in which the lint was stabilized.
        since: &'static str,
    },

    /// The lint is deprecated and no longer recommended for use.
    Deprecated {
        /// The version in which the lint was deprecated.
        since: &'static str,

        /// The reason why the lint has been deprecated.
        ///
        /// This should explain why the lint has been deprecated and if there's a replacement lint that users
        /// can use instead.
        reason: &'static str,
    },

    /// The lint has been removed and can no longer be used.
    Removed {
        /// The version in which the lint was removed.
        since: &'static str,

        /// The reason why the lint has been removed.
        reason: &'static str,
    },
}

impl LintStatus {
    pub const fn preview(since: &'static str) -> Self {
        LintStatus::Preview { since }
    }

    pub const fn stable(since: &'static str) -> Self {
        LintStatus::Stable { since }
    }

    pub const fn deprecated(since: &'static str, reason: &'static str) -> Self {
        LintStatus::Deprecated { since, reason }
    }

    pub const fn removed(since: &'static str, reason: &'static str) -> Self {
        LintStatus::Removed { since, reason }
    }

    pub const fn is_removed(&self) -> bool {
        matches!(self, LintStatus::Removed { .. })
    }

    pub const fn is_deprecated(&self) -> bool {
        matches!(self, LintStatus::Deprecated { .. })
    }
}

/// Declares a lint rule with the given metadata.
///
/// ```rust
/// use red_knot_python_semantic::declare_lint;
/// use red_knot_python_semantic::lint::{LintStatus, Level};
///
/// declare_lint! {
///     /// ## What it does
///     /// Checks for references to names that are not defined.
///     ///
///     /// ## Why is this bad?
///     /// Using an undefined variable will raise a `NameError` at runtime.
///     ///
///     /// ## Example
///     ///
///     /// ```python
///     /// print(x)  # NameError: name 'x' is not defined
///     /// ```
///     pub(crate) static UNRESOLVED_REFERENCE = {
///         summary: "detects references to names that are not defined",
///         status: LintStatus::preview("1.0.0"),
///         default_level: Level::Warn,
///     }
/// }
/// ```
#[macro_export]
macro_rules! declare_lint {
    (
        $(#[doc = $doc:literal])+
        $vis: vis static $name: ident = {
            summary: $summary: literal,
            status: $status: expr,
            // Optional properties
            $( $key:ident: $value:expr, )*
        }
    ) => {
        $( #[doc = $doc] )+
        #[allow(clippy::needless_update)]
        $vis static $name: $crate::lint::LintMetadata = $crate::lint::LintMetadata {
            name: ruff_db::diagnostic::LintName::of(ruff_macros::kebab_case!($name)),
            summary: $summary,
            raw_documentation: concat!($($doc, '\n',)+),
            status: $status,
            file: file!(),
            line: line!(),
            $( $key: $value, )*
            ..$crate::lint::lint_metadata_defaults()
        };
    };
}

/// A unique identifier for a lint rule.
///
/// Implements `PartialEq`, `Eq`, and `Hash` based on the `LintMetadata` pointer
/// for fast comparison and lookup.
#[derive(Debug, Clone, Copy)]
pub struct LintId {
    definition: &'static LintMetadata,
}

impl LintId {
    pub const fn of(definition: &'static LintMetadata) -> Self {
        LintId { definition }
    }
}

impl PartialEq for LintId {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.definition, other.definition)
    }
}

impl Eq for LintId {}

impl std::hash::Hash for LintId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::ptr::hash(self.definition, state);
    }
}

impl std::ops::Deref for LintId {
    type Target = LintMetadata;

    fn deref(&self) -> &Self::Target {
        self.definition
    }
}

#[derive(Default, Debug)]
pub struct LintRegistryBuilder {
    /// Registered lints that haven't been removed.
    lints: Vec<LintId>,

    /// Lints indexed by name, including aliases and removed rules.
    by_name: FxHashMap<&'static str, LintEntry>,
}

impl LintRegistryBuilder {
    #[track_caller]
    pub fn register_lint(&mut self, lint: &'static LintMetadata) {
        assert_eq!(
            self.by_name.insert(&*lint.name, lint.into()),
            None,
            "duplicate lint registration for '{name}'",
            name = lint.name
        );

        if !lint.status.is_removed() {
            self.lints.push(LintId::of(lint));
        }
    }

    #[track_caller]
    pub fn register_alias(&mut self, from: LintName, to: &'static LintMetadata) {
        let target = match self.by_name.get(to.name.as_str()) {
            Some(LintEntry::Lint(target) | LintEntry::Removed(target)) => target,
            Some(LintEntry::Alias(target)) => {
                panic!(
                    "lint alias {from} -> {to:?} points to another alias {target:?}",
                    target = target.name()
                )
            }
            None => panic!(
                "lint alias {from} -> {to} points to non-registered lint",
                to = to.name
            ),
        };

        assert_eq!(
            self.by_name
                .insert(from.as_str(), LintEntry::Alias(*target)),
            None,
            "duplicate lint registration for '{from}'",
        );
    }

    pub fn build(self) -> LintRegistry {
        LintRegistry {
            lints: self.lints,
            by_name: self.by_name,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct LintRegistry {
    lints: Vec<LintId>,
    by_name: FxHashMap<&'static str, LintEntry>,
}

impl LintRegistry {
    /// Looks up a lint by its name.
    pub fn get(&self, code: &str) -> Result<LintId, GetLintError> {
        match self.by_name.get(code) {
            Some(LintEntry::Lint(metadata)) => Ok(*metadata),
            Some(LintEntry::Alias(lint)) => {
                if lint.status.is_removed() {
                    Err(GetLintError::Removed(lint.name()))
                } else {
                    Ok(*lint)
                }
            }
            Some(LintEntry::Removed(lint)) => Err(GetLintError::Removed(lint.name())),
            None => {
                if let Some(without_prefix) = DiagnosticId::strip_category(code) {
                    if let Some(entry) = self.by_name.get(without_prefix) {
                        return Err(GetLintError::PrefixedWithCategory {
                            prefixed: code.to_string(),
                            suggestion: entry.id().name.to_string(),
                        });
                    }
                }

                Err(GetLintError::Unknown(code.to_string()))
            }
        }
    }

    /// Returns all registered, non-removed lints.
    pub fn lints(&self) -> &[LintId] {
        &self.lints
    }

    /// Returns an iterator over all known aliases and to their target lints.
    ///
    /// This iterator includes aliases that point to removed lints.
    pub fn aliases(&self) -> impl Iterator<Item = (LintName, LintId)> + '_ {
        self.by_name.iter().filter_map(|(key, value)| {
            if let LintEntry::Alias(alias) = value {
                Some((LintName::of(key), *alias))
            } else {
                None
            }
        })
    }

    /// Iterates over all removed lints.
    pub fn removed(&self) -> impl Iterator<Item = LintId> + '_ {
        self.by_name.iter().filter_map(|(_, value)| {
            if let LintEntry::Removed(metadata) = value {
                Some(*metadata)
            } else {
                None
            }
        })
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum GetLintError {
    /// The name maps to this removed lint.
    #[error("lint `{0}` has been removed")]
    Removed(LintName),

    /// No lint with the given name is known.
    #[error("unknown lint `{0}`")]
    Unknown(String),

    /// The name uses the full qualified diagnostic id `lint:<rule>` instead of just `rule`.
    /// The String is the name without the `lint:` category prefix.
    #[error("unknown lint `{prefixed}`. Did you mean `{suggestion}`?")]
    PrefixedWithCategory {
        prefixed: String,
        suggestion: String,
    },
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LintEntry {
    /// An existing lint rule. Can be in preview, stable or deprecated.
    Lint(LintId),
    /// A lint rule that has been removed.
    Removed(LintId),
    Alias(LintId),
}

impl LintEntry {
    fn id(self) -> LintId {
        match self {
            LintEntry::Lint(id) => id,
            LintEntry::Removed(id) => id,
            LintEntry::Alias(id) => id,
        }
    }
}

impl From<&'static LintMetadata> for LintEntry {
    fn from(metadata: &'static LintMetadata) -> Self {
        if metadata.status.is_removed() {
            LintEntry::Removed(LintId::of(metadata))
        } else {
            LintEntry::Lint(LintId::of(metadata))
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleSelection {
    /// Map with the severity for each enabled lint rule.
    ///
    /// If a rule isn't present in this map, then it should be considered disabled.
    lints: FxHashMap<LintId, (Severity, LintSource)>,
}

impl RuleSelection {
    /// Creates a new rule selection from all known lints in the registry that are enabled
    /// according to their default severity.
    pub fn from_registry(registry: &LintRegistry) -> Self {
        let lints = registry
            .lints()
            .iter()
            .filter_map(|lint| {
                Severity::try_from(lint.default_level())
                    .ok()
                    .map(|severity| (*lint, (severity, LintSource::Default)))
            })
            .collect();

        RuleSelection { lints }
    }

    /// Returns an iterator over all enabled lints.
    pub fn enabled(&self) -> impl Iterator<Item = LintId> + '_ {
        self.lints.keys().copied()
    }

    /// Returns an iterator over all enabled lints and their severity.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (LintId, Severity)> + '_ {
        self.lints
            .iter()
            .map(|(&lint, &(severity, _))| (lint, severity))
    }

    /// Returns the configured severity for the lint with the given id or `None` if the lint is disabled.
    pub fn severity(&self, lint: LintId) -> Option<Severity> {
        self.lints.get(&lint).map(|(severity, _)| *severity)
    }

    /// Returns `true` if the `lint` is enabled.
    pub fn is_enabled(&self, lint: LintId) -> bool {
        self.severity(lint).is_some()
    }

    /// Enables `lint` and configures with the given `severity`.
    ///
    /// Overrides any previous configuration for the lint.
    pub fn enable(&mut self, lint: LintId, severity: Severity, source: LintSource) {
        self.lints.insert(lint, (severity, source));
    }

    /// Disables `lint` if it was previously enabled.
    pub fn disable(&mut self, lint: LintId) {
        self.lints.remove(&lint);
    }
}

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
pub enum LintSource {
    /// The user didn't enable the rule explicitly, instead it's enabled by default.
    #[default]
    Default,

    /// The rule was enabled by using a CLI argument
    Cli,

    /// The rule was enabled in a configuration file.
    File,
}
